//! Port of `clients/portal/src/launchables/state.ts`: the launchables ADT.
//! korrid outcomes are converted into this ADT at the seam; the surface model
//! never inspects an RPC payload directly.

use super::fold::{fold_game_copies, GameCopy};
use crate::{
    game_routes::SelectedGameLaunchOutcome, launcher::LocalGame, stream_play::StreamPlayEnded,
    ActiveSession, CatalogSnapshotOutcome, Game, InitialHandoff, LocalGamesListOutcome,
    PeerListOutcome, PeerListState, PendingLaunchPhase, RpcFailure, SessionEndOutcome,
    SessionPrepared, SessionStatus, SessionStatusOutcome,
};

#[derive(Clone, Debug)]
pub struct SessionCatalogEntry {
    pub entry: Box<Entry>,
    /// Restore the catalog's order when the session ends between reads.
    pub position: usize,
}

#[derive(Clone, Debug)]
pub enum Entry {
    NowPlaying {
        session: ActiveSession,
        /// Source evidence for this launch only, never a stale entry.
        local_catalog_game: Option<Game>,
        /// The current catalog card, moved here rather than duplicated.
        catalog_entry: Option<SessionCatalogEntry>,
    },
    LocalGame {
        game: LocalGame,
        alternatives: Vec<GameCopy>,
    },
    Game {
        game: Game,
        alternatives: Vec<GameCopy>,
    },
}

impl Entry {
    pub fn key(&self) -> String {
        if let Some(entry) = self.catalog_entry() {
            return entry.key();
        }
        match self {
            Self::NowPlaying { session, .. } => format!("now-playing:{}", session.launch_id),
            Self::LocalGame { game, .. } => format!("local-game:{}", game.id),
            Self::Game { game, .. } => match &game.host {
                None => format!("game:{}", game.id),
                Some(host) => format!("game:{host}:{}", game.id),
            },
        }
    }
    pub fn catalog_entry(&self) -> Option<&Entry> {
        match self {
            Self::NowPlaying {
                catalog_entry: Some(catalog),
                ..
            } => Some(&catalog.entry),
            _ => None,
        }
    }
    pub fn alternatives(&self) -> &[GameCopy] {
        if let Some(entry) = self.catalog_entry() {
            return entry.alternatives();
        }
        match self {
            Self::LocalGame { alternatives, .. } | Self::Game { alternatives, .. } => alternatives,
            Self::NowPlaying { .. } => &[],
        }
    }
    pub fn session(&self) -> Option<&ActiveSession> {
        match self {
            Self::NowPlaying { session, .. } => Some(session),
            _ => None,
        }
    }
    pub fn is_now_playing(&self) -> bool {
        matches!(self, Self::NowPlaying { .. })
    }
    /// The catalog `Game` of a `game` entry and every remote alternative.
    pub fn catalog_games(&self) -> impl Iterator<Item = &Game> {
        let own = match self.catalog_entry().unwrap_or(self) {
            Self::Game { game, .. } => Some(game),
            _ => None,
        };
        own.into_iter()
            .chain(self.alternatives().iter().filter_map(GameCopy::remote))
    }
    pub fn for_copy(copy: &GameCopy) -> Entry {
        match copy {
            GameCopy::Remote(game) => Entry::Game {
                game: game.clone(),
                alternatives: Vec::new(),
            },
            GameCopy::Local(game) => Entry::LocalGame {
                game: game.clone(),
                alternatives: Vec::new(),
            },
        }
    }
}

pub fn entry_label(entry: &Entry) -> String {
    match entry {
        Entry::NowPlaying { session, .. } => session
            .title
            .clone()
            .or_else(|| session.game_id.clone())
            .unwrap_or_else(|| "Current session".into()),
        Entry::LocalGame { game, .. } => game.title.clone(),
        Entry::Game { game, .. } => game.title.clone(),
    }
}

/// Linux status omits host, while its catalog uses config.label. Require
/// source.isLocal and the game id; an explicit session host must still match
/// that copy, never a same-id peer.
fn local_catalog_game_for_session(session: &ActiveSession, entries: &[Entry]) -> Option<Game> {
    let matches = |game: &Game| {
        game.source.is_local
            && Some(&game.id) == session.game_id.as_ref()
            && (session.host.is_none() || game.host == session.host)
    };
    for entry in entries {
        if let Some(game) = entry.catalog_games().find(|game| matches(game)) {
            return Some(game.clone());
        }
        if let Entry::NowPlaying {
            session: other,
            local_catalog_game: Some(game),
            ..
        } = entry
        {
            if other.launch_id == session.launch_id && matches(game) {
                return Some(game.clone());
            }
        }
    }
    None
}

pub fn is_local_catalog_session(session: &ActiveSession, entries: &[Entry]) -> bool {
    local_catalog_game_for_session(session, entries).is_some()
}

/// Native peer status qualifies the session's host. Local ACKs can also
/// carry a host label, so retain the exact local source evidence first.
fn has_far_session(entries: &[Entry]) -> bool {
    entries.iter().any(|entry| {
        entry.session().is_some_and(|session| {
            session.host.is_some() && !is_local_catalog_session(session, entries)
        })
    })
}

// Stream sessions carry no GameIdentity. A qualified host plus game id
// identifies a remote copy; hostless native status can only name a local copy.
fn catalog_matches_session(session: &ActiveSession, entry: &Entry) -> bool {
    entry.catalog_games().any(|game| {
        Some(&game.id) == session.game_id.as_ref()
            && match &session.host {
                Some(host) => game.host.as_ref() == Some(host),
                None => game.source.is_local,
            }
    }) || matches!(entry, Entry::LocalGame { game, .. }
        if session.host.is_none() && Some(&game.id) == session.game_id.as_ref())
}

fn session_entry(session: &ActiveSession, entries: &mut Vec<Entry>, previous: &[Entry]) -> Entry {
    // Old catalog facts establish locality only for the exact observed launch.
    let previous_game = if previous.iter().any(|entry| {
        entry
            .session()
            .is_some_and(|other| other.launch_id == session.launch_id)
    }) && !is_local_catalog_session(session, entries)
    {
        local_catalog_game_for_session(session, previous)
    } else {
        None
    };
    let catalog_entry = entries
        .iter()
        .position(|entry| catalog_matches_session(session, entry))
        .map(|position| SessionCatalogEntry {
            entry: Box::new(entries.remove(position)),
            position,
        });
    Entry::NowPlaying {
        session: session.clone(),
        local_catalog_game: previous_game,
        catalog_entry,
    }
}

/// The Linux executor reports idle as Err(SessionCompleted/NoActiveSession).
/// This is session truth, unlike an observation failure.
pub fn is_authoritative_session_status(status: &SessionStatusOutcome) -> bool {
    match status {
        SessionStatusOutcome::Ok(payload) => payload.observation_failure.is_none(),
        SessionStatusOutcome::Err(failure) => {
            failure.code == "SessionCompleted" || failure.code == "NoActiveSession"
        }
    }
}

fn status_failure(status: &SessionStatusOutcome) -> Option<&RpcFailure> {
    match status {
        SessionStatusOutcome::Ok(payload) => payload.observation_failure.as_ref(),
        SessionStatusOutcome::Err(failure) => Some(failure),
    }
}

/// The exact game an in-flight start belongs to, so a later failure keeps it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LaunchSubject {
    /// Raw content identity, absent when the daemon does not know it.
    pub id: Option<String>,
    /// Existing presented entry key captured from the exact selected copy.
    pub entry_id: Option<String>,
    pub title: String,
}

/// A notice always states what it is about.
#[derive(Clone, Debug, PartialEq)]
pub enum LaunchNotice {
    Catalog {
        message: String,
    },
    Launch {
        message: String,
        subject: Option<LaunchSubject>,
    },
    Stop {
        message: String,
    },
    Action {
        message: String,
    },
}

impl LaunchNotice {
    pub fn message(&self) -> &str {
        match self {
            Self::Catalog { message }
            | Self::Launch { message, .. }
            | Self::Stop { message }
            | Self::Action { message } => message,
        }
    }
    fn launch(message: impl Into<String>) -> Self {
        Self::Launch {
            message: message.into(),
            subject: None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Content {
    pub stream_play_launch_id: Option<String>,
    pub stream_play_ended: Option<StreamPlayEnded>,
    pub entries: Vec<Entry>,
    pub notice: Option<LaunchNotice>,
    /// Actual failed daemon observation; entries retain only last-known
    /// identity.
    pub unavailable_session_status: Option<SessionStatusOutcome>,
}

impl Content {
    fn with_entries(entries: Vec<Entry>, notice: Option<LaunchNotice>) -> Self {
        Self {
            entries,
            notice,
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ChoicePhase {
    Pending(PendingLaunchPhase),
    Waiting,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CancelState {
    Idle,
    Sent,
    Retry,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PendingLaunchChoice {
    pub launch_id: String,
    pub game_id: Option<String>,
    pub title: String,
    pub phase: ChoicePhase,
    pub cancel: CancelState,
}

#[derive(Clone, Debug)]
pub enum Phase {
    Ready,
    Preparing {
        title: String,
        subject: Option<LaunchSubject>,
    },
    Launching {
        return_launch_id: Option<String>,
        title: String,
        subject: Option<LaunchSubject>,
    },
    Stopping {
        launch_id: String,
    },
    Starting {
        title: String,
        subject: LaunchSubject,
        launch_id: Option<String>,
        cancelling: bool,
        cancel_retry_available: Option<bool>,
    },
    Choosing {
        choices: Vec<PendingLaunchChoice>,
    },
    /// The content's notice is always present.
    Recovery,
}

#[derive(Clone, Debug)]
pub struct Loaded {
    pub phase: Phase,
    pub content: Content,
}

#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)] // Loading is brief; Loaded is the state.
pub enum LaunchablesState {
    Loading,
    Loaded(Loaded),
}

fn ready(content: Content) -> LaunchablesState {
    LaunchablesState::Loaded(Loaded {
        phase: Phase::Ready,
        content,
    })
}

fn subject_of(phase: &Phase) -> Option<LaunchSubject> {
    match phase {
        Phase::Preparing { subject, .. } | Phase::Launching { subject, .. } => subject.clone(),
        Phase::Starting { subject, .. } => Some(subject.clone()),
        _ => None,
    }
}

/// Return to browsing while keeping the failed start's own identity.
fn ready_from(state: Loaded, message: impl Into<String>) -> LaunchablesState {
    let message = message.into();
    let notice = match &state.phase {
        Phase::Stopping { .. } => LaunchNotice::Stop { message },
        phase => LaunchNotice::Launch {
            message,
            subject: subject_of(phase),
        },
    };
    ready(Content {
        stream_play_launch_id: None,
        stream_play_ended: None,
        entries: state.content.entries,
        notice: Some(notice),
        unavailable_session_status: state.content.unavailable_session_status,
    })
}

fn retire_failed_far_session(mut state: Loaded, failure: &RpcFailure) -> LaunchablesState {
    state.content.entries = without_sessions(&state.content.entries);
    state.content.unavailable_session_status = None;
    ready_from(state, format!("{}: {}", failure.code, failure.message))
}

pub fn launch_subject_for_game(game: &Game, entries: &[Entry]) -> LaunchSubject {
    let matches = |candidate: &Game| {
        candidate.id == game.id
            && candidate.host == game.host
            && candidate.source.is_local == game.source.is_local
    };
    let entry = entries.iter().find(|entry| {
        matches!(entry, Entry::Game { game, .. } if matches(game))
            || (!entry.is_now_playing()
                && entry
                    .alternatives()
                    .iter()
                    .any(|copy| copy.remote().is_some_and(&matches)))
    });
    LaunchSubject {
        id: Some(game.id.clone()),
        entry_id: entry.map(Entry::key),
        title: game.title.clone(),
    }
}

fn same_json<T: serde::Serialize>(left: &T, right: &T) -> bool {
    serde_json::to_value(left).ok() == serde_json::to_value(right).ok()
}

fn update_stream_play(content: &mut Content, status: &SessionStatus) {
    if content.stream_play_launch_id != status.stream_play_launch_id
        || !same_json(&content.stream_play_ended, &status.stream_play_ended)
    {
        content.stream_play_launch_id = status.stream_play_launch_id.clone();
        content.stream_play_ended = status.stream_play_ended.clone();
    }
}

pub(super) fn without_sessions(entries: &[Entry]) -> Vec<Entry> {
    let mut catalog: Vec<Entry> = entries
        .iter()
        .filter(|entry| !entry.is_now_playing())
        .cloned()
        .collect();
    for entry in entries {
        if let Entry::NowPlaying {
            catalog_entry: Some(saved),
            ..
        } = entry
        {
            catalog.insert(saved.position.min(catalog.len()), (*saved.entry).clone());
        }
    }
    catalog
}

fn has_local_catalog(entries: &[Entry]) -> bool {
    entries.iter().any(|entry| {
        matches!(entry, Entry::Game { game, .. } if game.source.is_local)
            || (!entry.is_now_playing()
                && entry
                    .alternatives()
                    .iter()
                    .any(|copy| copy.remote().is_some_and(|game| game.source.is_local)))
    })
}

/// The catalog notice in plain words. `Problem.reason` is finished copy
/// (contract `SurfaceStatus`), so codes and game ids stay in korrid's RPC
/// results and the CLI; the surface gets one short sentence per kind.
#[derive(Default)]
struct CatalogTrouble {
    missing_runner: usize,
    missing_files: usize,
    sentences: Vec<String>,
}

impl CatalogTrouble {
    fn add(&mut self, sentence: String) {
        if !self.sentences.contains(&sentence) {
            self.sentences.push(sentence);
        }
    }

    /// A failure a game source reported beside its games. One game's own
    /// failure counts only when `counts_games`: nothing is listed at all.
    fn listed_failure(&mut self, code: &str, host: Option<&str>, counts_games: bool) {
        match code {
            // A warning about a setting; the game still launches.
            "LaunchSettingUnsupported" => {}
            "LocalRouteUnavailable" if counts_games => self.missing_runner += 1,
            "LocalRomMissing" if counts_games => self.missing_files += 1,
            "LocalRouteUnavailable" | "LocalRomMissing" => {}
            code if code.starts_with("LocalConfig") => {
                self.add("This device's library settings couldn't be read.".into())
            }
            _ => self.add(match host {
                Some(host) => format!("Korri couldn't read the games on {host}."),
                None => "Korri couldn't read some of this device's games.".into(),
            }),
        }
    }

    fn message(self) -> Option<String> {
        let mut parts = Vec::new();
        match self.missing_runner {
            0 => {}
            1 => parts.push("1 game needs a runner that isn't installed.".to_owned()),
            n => parts.push(format!("{n} games need a runner that isn't installed.")),
        }
        match self.missing_files {
            0 => {}
            1 => parts.push("1 game is missing its files.".to_owned()),
            n => parts.push(format!("{n} games are missing their files.")),
        }
        parts.extend(self.sentences);
        (!parts.is_empty()).then(|| parts.join(" "))
    }
}

impl LaunchablesState {
    pub fn loaded(&self) -> Option<&Loaded> {
        match self {
            Self::Loaded(loaded) => Some(loaded),
            Self::Loading => None,
        }
    }
    pub fn entries(&self) -> &[Entry] {
        self.loaded().map_or(&[], |loaded| &loaded.content.entries)
    }
    pub fn is_ready(&self) -> bool {
        matches!(
            self,
            Self::Loaded(Loaded {
                phase: Phase::Ready,
                ..
            })
        )
    }

    /// Fold Korri-owned game sources into one state.
    pub fn from_sources(
        korrid: &CatalogSnapshotOutcome,
        session: Option<&SessionStatusOutcome>,
        local_games: Option<&LocalGamesListOutcome>,
        previous_entries: &[Entry],
        peers: Option<&PeerListOutcome>,
    ) -> LaunchablesState {
        let mut entries = Vec::new();
        let mut trouble = CatalogTrouble::default();
        let local_catalog: &[LocalGame] = match local_games {
            Some(LocalGamesListOutcome::Ok(payload)) => &payload.games,
            _ => &[],
        };
        let remote_catalog: &[Game] = match korrid {
            CatalogSnapshotOutcome::Ok(payload) => &payload.games,
            CatalogSnapshotOutcome::Err(_) => &[],
        };
        for folded in fold_game_copies(local_catalog, remote_catalog) {
            entries.push(match folded.primary {
                GameCopy::Local(game) => Entry::LocalGame {
                    game,
                    alternatives: folded.alternatives,
                },
                GameCopy::Remote(game) => Entry::Game {
                    game,
                    alternatives: folded.alternatives,
                },
            });
        }
        // Source evidence can outlive a catalog read, but never restores its games.
        if let Some(status @ SessionStatusOutcome::Ok(payload)) = session {
            if is_authoritative_session_status(status) {
                if let Some(active) = &payload.active {
                    let entry = session_entry(active, &mut entries, previous_entries);
                    entries.insert(0, entry);
                }
            }
        }
        // A game that cannot run must not hide the games that can: its own
        // failure reaches the notice only when no game is listed at all.
        let nothing_listed = local_catalog.is_empty() && remote_catalog.is_empty();
        match local_games {
            Some(LocalGamesListOutcome::Ok(payload)) => {
                for failure in payload.failures.iter().flatten() {
                    trouble.listed_failure(&failure.code, None, nothing_listed);
                }
            }
            // An absent inventory capability is not a failed read.
            Some(LocalGamesListOutcome::Err(failure)) if failure.code != "OperationUnsupported" => {
                trouble.add("Korri couldn't read this device's games.".into());
            }
            _ => {}
        }
        if let Some(PeerListOutcome::Err(_)) = peers {
            trouble.add("Korri couldn't list your other devices.".into());
        }
        match korrid {
            CatalogSnapshotOutcome::Ok(payload) => {
                // A partial peer read must not block healthy games; keep the
                // notice only when no game source succeeded.
                if nothing_listed {
                    for failure in payload.failures.iter().flatten() {
                        trouble.listed_failure(&failure.code, Some(&failure.host), true);
                    }
                }
            }
            CatalogSnapshotOutcome::Err(_) => {
                trouble.add("Korri couldn't read the game library.".into());
            }
        }
        let notice = trouble
            .message()
            .map(|message| LaunchNotice::Catalog { message });
        if let Some(session) = session {
            if let SessionStatusOutcome::Err(failure) = session {
                if failure.code == "HostLaunchFailed" && has_far_session(previous_entries) {
                    return retire_failed_far_session(
                        Loaded {
                            phase: Phase::Ready,
                            content: Content::with_entries(entries, notice),
                        },
                        failure,
                    );
                }
            }
            if !is_authoritative_session_status(session) {
                let known = previous_entries.iter().find(|entry| {
                    entry
                        .session()
                        .is_some_and(|session| is_local_catalog_session(session, previous_entries))
                });
                if let Some(Entry::NowPlaying { session: known, .. }) = known {
                    if !entries.iter().any(Entry::is_now_playing) {
                        let entry = session_entry(known, &mut entries, previous_entries);
                        entries.insert(0, entry);
                    }
                    return ready(Content {
                        unavailable_session_status: Some(session.clone()),
                        ..Content::with_entries(entries, notice)
                    });
                }
            }
        }
        let recovered = Self::recover_startup(Content::with_entries(entries, notice), session, &[]);
        // An empty read is provisional while known peers are connecting.
        if let LaunchablesState::Loaded(Loaded {
            phase: Phase::Ready,
            content,
        }) = &recovered
        {
            if content.entries.is_empty()
                && content.notice.is_none()
                && matches!(peers, Some(PeerListOutcome::Ok(list)) if list.peers.iter().any(|peer| peer.state == PeerListState::Loading))
            {
                return LaunchablesState::Loading;
            }
        }
        recovered
    }

    /// Daemon facts, never current focus or surface host history, define fresh
    /// recovery. `state` is a Ready state's content.
    pub fn recover_startup(
        mut state: Content,
        status: Option<&SessionStatusOutcome>,
        previous_choices: &[PendingLaunchChoice],
    ) -> LaunchablesState {
        let Some(status) = status else {
            return ready(state);
        };
        if let SessionStatusOutcome::Ok(payload) = status {
            update_stream_play(&mut state, payload);
        }
        let failure = status_failure(status).cloned();
        let payload = match status {
            SessionStatusOutcome::Err(error) => {
                if error.code == "HostLaunchFailed" && has_far_session(&state.entries) {
                    return retire_failed_far_session(
                        Loaded {
                            phase: Phase::Ready,
                            content: state,
                        },
                        error,
                    );
                }
                return if !is_authoritative_session_status(status)
                    && has_local_catalog(&state.entries)
                {
                    state.notice = Some(LaunchNotice::launch(format!(
                        "{}: {}",
                        error.code, error.message
                    )));
                    LaunchablesState::Loaded(Loaded {
                        phase: Phase::Recovery,
                        content: state,
                    })
                } else {
                    ready(state)
                };
            }
            SessionStatusOutcome::Ok(payload) => payload,
        };
        let active = if is_authoritative_session_status(status) {
            payload.active.as_ref()
        } else {
            None
        };
        let returned_or_stopping = active.is_some_and(|active| {
            matches!(
                active.phase.as_deref(),
                Some("frozen" | "focus-failed" | "stopping")
            )
        });
        let mut candidates: Vec<(String, Option<String>, ChoicePhase)> = Vec::new();
        for pending in payload.pending_launches.iter().flatten() {
            let excluded = active.is_some_and(|active| {
                pending.session.launch_id == active.launch_id
                    && (returned_or_stopping
                        || matches!(
                            active.initial_handoff,
                            Some(InitialHandoff::Observed | InitialHandoff::Recovered)
                        ))
            });
            if excluded {
                continue;
            }
            candidates.retain(|(launch_id, ..)| *launch_id != pending.session.launch_id);
            candidates.push((
                pending.session.launch_id.clone(),
                Some(pending.session.game_id.clone()),
                ChoicePhase::Pending(pending.phase),
            ));
        }
        if let Some(active) = active {
            if active.initial_handoff == Some(InitialHandoff::Waiting)
                && !returned_or_stopping
                && !candidates
                    .iter()
                    .any(|(launch_id, ..)| *launch_id == active.launch_id)
            {
                candidates.push((
                    active.launch_id.clone(),
                    active.game_id.clone(),
                    ChoicePhase::Waiting,
                ));
            }
        }
        let local_games: Vec<Game> = state
            .entries
            .iter()
            .flat_map(|entry| entry.catalog_games().cloned().collect::<Vec<_>>())
            .collect();
        let mut choices: Vec<PendingLaunchChoice> = candidates
            .into_iter()
            .map(|(launch_id, game_id, phase)| {
                let title = local_games
                    .iter()
                    .find(|game| game.source.is_local && Some(&game.id) == game_id.as_ref())
                    .map(|game| game.title.clone())
                    .or_else(|| {
                        active
                            .filter(|active| active.launch_id == launch_id)
                            .and_then(|active| active.title.clone())
                    })
                    .or_else(|| game_id.clone())
                    .unwrap_or_else(|| "live session".into());
                let cancel = if phase == ChoicePhase::Pending(PendingLaunchPhase::Cancelling) {
                    CancelState::Sent
                } else {
                    previous_choices
                        .iter()
                        .find(|choice| choice.launch_id == launch_id)
                        .map_or(CancelState::Idle, |choice| choice.cancel)
                };
                PendingLaunchChoice {
                    launch_id,
                    game_id,
                    title,
                    phase,
                    cancel,
                }
            })
            .collect();
        choices.sort_by(|left, right| left.launch_id.cmp(&right.launch_id));
        if choices.len() > 1 {
            if let Some(failure) = &failure {
                state.notice = Some(LaunchNotice::launch(failure.message.clone()));
            }
            return LaunchablesState::Loaded(Loaded {
                phase: Phase::Choosing { choices },
                content: state,
            });
        }
        let Some(choice) = choices.into_iter().next() else {
            // Stops from inputd or the owner CLI use the same busy state as
            // the surface's End action, without starting another stop request.
            if let Some(active) =
                active.filter(|active| active.phase.as_deref() == Some("stopping"))
            {
                return LaunchablesState::Loaded(Loaded {
                    phase: Phase::Stopping {
                        launch_id: active.launch_id.clone(),
                    },
                    content: state,
                });
            }
            return match failure {
                Some(failure) => {
                    state.notice = Some(LaunchNotice::launch(format!(
                        "{}: {}",
                        failure.code, failure.message
                    )));
                    LaunchablesState::Loaded(Loaded {
                        phase: Phase::Recovery,
                        content: state,
                    })
                }
                None => ready(state),
            };
        };
        let local_game = local_games
            .iter()
            .find(|game| game.source.is_local && Some(&game.id) == choice.game_id.as_ref());
        let subject = match local_game {
            None => LaunchSubject {
                id: choice.game_id.clone(),
                entry_id: None,
                title: choice.title.clone(),
            },
            Some(game) => launch_subject_for_game(game, &state.entries),
        };
        let cancelling_phase = choice.phase == ChoicePhase::Pending(PendingLaunchPhase::Cancelling);
        let mut cancel_retry_available = (choice.cancel == CancelState::Retry).then_some(true);
        if let Some(failure) = &failure {
            state.notice = Some(LaunchNotice::launch(failure.message.clone()));
            cancel_retry_available = Some(!cancelling_phase && choice.cancel != CancelState::Sent);
        }
        LaunchablesState::Loaded(Loaded {
            phase: Phase::Starting {
                title: choice.title.clone(),
                subject,
                launch_id: Some(choice.launch_id.clone()),
                cancelling: cancelling_phase
                    || choice.cancel == CancelState::Sent
                    || choice.cancel == CancelState::Retry,
                cancel_retry_available,
            },
            content: state,
        })
    }

    /// None when nothing changes.
    pub fn begin_pending_cancellation(&self, launch_id: &str) -> Option<LaunchablesState> {
        let LaunchablesState::Loaded(Loaded {
            phase: Phase::Choosing { choices },
            content,
        }) = self
        else {
            return None;
        };
        if !choices.iter().any(|choice| {
            choice.launch_id == launch_id
                && (choice.cancel == CancelState::Retry
                    || (choice.cancel == CancelState::Idle
                        && choice.phase != ChoicePhase::Pending(PendingLaunchPhase::Cancelling)))
        }) {
            return None;
        }
        Some(LaunchablesState::Loaded(Loaded {
            phase: Phase::Choosing {
                choices: choices
                    .iter()
                    .map(|choice| {
                        let mut choice = choice.clone();
                        if choice.launch_id == launch_id {
                            choice.cancel = CancelState::Sent;
                        }
                        choice
                    })
                    .collect(),
            },
            content: content.clone(),
        }))
    }

    pub fn with_pending_cancellation_outcome(
        self,
        launch_id: &str,
        outcome: &SessionEndOutcome,
    ) -> LaunchablesState {
        let LaunchablesState::Loaded(mut loaded) = self else {
            return self;
        };
        let choosing = matches!(&loaded.phase, Phase::Choosing { choices } if choices.iter().any(|choice| choice.launch_id == launch_id && choice.cancel == CancelState::Sent));
        let single = matches!(&loaded.phase, Phase::Starting { launch_id: Some(id), cancelling: true, .. } if id == launch_id);
        if !choosing && !single {
            return LaunchablesState::Loaded(loaded);
        }
        match outcome {
            SessionEndOutcome::Ok(_) => {
                loaded.content.notice = None;
                LaunchablesState::Loaded(loaded)
            }
            SessionEndOutcome::Err(failure)
                if ["StaleLaunchIdentity", "NoActiveSession", "SessionCompleted"]
                    .contains(&failure.code.as_str()) =>
            {
                LaunchablesState::Loaded(loaded)
            }
            SessionEndOutcome::Err(failure) => {
                if matches!(loaded.phase, Phase::Starting { .. }) {
                    return LaunchablesState::Loaded(loaded).startup_problem(&failure.message);
                }
                loaded.content.notice = Some(LaunchNotice::launch(failure.message.clone()));
                if let Phase::Choosing { choices } = &mut loaded.phase {
                    for choice in choices.iter_mut() {
                        if choice.launch_id == launch_id {
                            choice.cancel = CancelState::Retry;
                        }
                    }
                }
                LaunchablesState::Loaded(loaded)
            }
        }
    }

    /// Replace the notice on a Ready state, leaving its entries alone.
    pub fn with_notice(self, message: impl Into<String>) -> LaunchablesState {
        match self {
            LaunchablesState::Loaded(mut loaded) if matches!(loaded.phase, Phase::Ready) => {
                loaded.content.notice = Some(LaunchNotice::Action {
                    message: message.into(),
                });
                LaunchablesState::Loaded(loaded)
            }
            other => other,
        }
    }

    pub fn begin_startup(self, game: &Game) -> LaunchablesState {
        match self {
            LaunchablesState::Loaded(mut loaded) if matches!(loaded.phase, Phase::Ready) => {
                let subject = launch_subject_for_game(game, &loaded.content.entries);
                loaded.content.notice = None;
                loaded.phase = Phase::Starting {
                    title: game.title.clone(),
                    subject,
                    launch_id: None,
                    cancelling: false,
                    cancel_retry_available: None,
                };
                LaunchablesState::Loaded(loaded)
            }
            other => other,
        }
    }

    pub fn with_startup_reservation(
        self,
        session: &SessionPrepared,
        game: &Game,
    ) -> LaunchablesState {
        match self {
            LaunchablesState::Loaded(Loaded {
                phase:
                    Phase::Starting {
                        title,
                        subject,
                        cancelling,
                        cancel_retry_available,
                        ..
                    },
                content,
            }) if subject.id.as_deref() == Some(session.game_id.as_str()) => {
                LaunchablesState::Loaded(Loaded {
                    phase: Phase::Starting {
                        title,
                        subject,
                        launch_id: Some(session.launch_id.clone()),
                        cancelling,
                        cancel_retry_available,
                    },
                    content,
                })
                .with_local_catalog_acknowledgement(session, game)
            }
            other => other,
        }
    }

    pub fn cancel_startup(self) -> LaunchablesState {
        match self {
            LaunchablesState::Loaded(Loaded {
                phase:
                    Phase::Starting {
                        title,
                        subject,
                        launch_id,
                        ..
                    },
                content,
            }) => LaunchablesState::Loaded(Loaded {
                phase: Phase::Starting {
                    title,
                    subject,
                    launch_id,
                    cancelling: true,
                    cancel_retry_available: Some(false),
                },
                content,
            }),
            other => other,
        }
    }

    pub fn startup_problem(self, message: &str) -> LaunchablesState {
        match self {
            LaunchablesState::Loaded(Loaded {
                phase:
                    Phase::Starting {
                        title,
                        subject,
                        launch_id,
                        cancelling,
                        ..
                    },
                mut content,
            }) => {
                content.notice = Some(LaunchNotice::Launch {
                    message: message.into(),
                    subject: Some(subject.clone()),
                });
                LaunchablesState::Loaded(Loaded {
                    phase: Phase::Starting {
                        title,
                        subject,
                        launch_id,
                        cancelling,
                        cancel_retry_available: Some(true),
                    },
                    content,
                })
            }
            other => other,
        }
    }

    pub fn fail_startup(self, message: &str, preserve_session: bool) -> LaunchablesState {
        match self {
            LaunchablesState::Loaded(mut loaded) => {
                let Phase::Starting { launch_id, .. } = &loaded.phase else {
                    return LaunchablesState::Loaded(loaded);
                };
                if !preserve_session {
                    let launch_id = launch_id.clone();
                    loaded.content.entries.retain(|entry| {
                        entry
                            .session()
                            .is_none_or(|session| Some(&session.launch_id) != launch_id.as_ref())
                    });
                }
                ready_from(loaded, message)
            }
            other => other,
        }
    }

    pub fn begin_preparing(self, title: &str, subject: Option<LaunchSubject>) -> LaunchablesState {
        match self {
            LaunchablesState::Loaded(mut loaded) if matches!(loaded.phase, Phase::Ready) => {
                loaded.content.notice = None;
                loaded.phase = Phase::Preparing {
                    title: title.into(),
                    subject,
                };
                LaunchablesState::Loaded(loaded)
            }
            other => other,
        }
    }

    pub fn begin_return(content: Content, session: &ActiveSession) -> LaunchablesState {
        let title = session
            .title
            .clone()
            .or_else(|| session.game_id.clone())
            .unwrap_or_else(|| "Live session".into());
        let entry_id = content
            .entries
            .iter()
            .find(|entry| {
                entry
                    .session()
                    .is_some_and(|other| other.launch_id == session.launch_id)
            })
            .map(Entry::key);
        LaunchablesState::Loaded(Loaded {
            phase: Phase::Launching {
                return_launch_id: Some(session.launch_id.clone()),
                title: title.clone(),
                subject: Some(LaunchSubject {
                    id: session.game_id.clone(),
                    entry_id,
                    title,
                }),
            },
            content,
        })
    }

    /// Resume locks the list as Launching, so a refusal folds from that case.
    pub fn with_resume_outcome(self, failure: Option<&RpcFailure>) -> LaunchablesState {
        match self {
            LaunchablesState::Loaded(loaded) if matches!(loaded.phase, Phase::Launching { .. }) => {
                match failure {
                    Some(failure)
                        if failure.code == "HostLaunchFailed"
                            && has_far_session(&loaded.content.entries) =>
                    {
                        retire_failed_far_session(loaded, failure)
                    }
                    None => LaunchablesState::Loaded(loaded),
                    Some(failure)
                        if matches!(
                            failure.code.as_str(),
                            "SessionCompleted" | "NoActiveSession"
                        ) =>
                    {
                        ready(Content::with_entries(
                            without_sessions(&loaded.content.entries),
                            None,
                        ))
                    }
                    Some(failure) => {
                        ready_from(loaded, format!("{}: {}", failure.code, failure.message))
                    }
                }
            }
            other => other,
        }
    }

    /// Prepare proves an owned launch exists; no native activity swap
    /// follows on Linux.
    pub fn with_local_catalog_prepare_outcome(
        self,
        outcome: &SelectedGameLaunchOutcome,
        game: &Game,
    ) -> LaunchablesState {
        match self {
            LaunchablesState::Loaded(loaded) if matches!(loaded.phase, Phase::Preparing { .. }) => {
                match outcome {
                    SelectedGameLaunchOutcome::Err(failure) => {
                        ready_from(loaded, format!("{}: {}", failure.code, failure.message))
                    }
                    SelectedGameLaunchOutcome::Ok(payload) => {
                        ready(Content::with_entries(loaded.content.entries, None))
                            .with_local_catalog_acknowledgement(&payload.session, game)
                    }
                }
            }
            other => other,
        }
    }

    /// An ACK owns its source evidence even if the next catalog read loses it.
    pub fn with_local_catalog_acknowledgement(
        self,
        session: &SessionPrepared,
        game: &Game,
    ) -> LaunchablesState {
        match self {
            LaunchablesState::Loaded(mut loaded) => {
                let active = ActiveSession {
                    netplay: None,
                    person_public_key: None,
                    stream_players: Vec::new(),
                    steps: None,
                    initial_handoff: None,
                    focus_ownership: None,
                    launch_id: session.launch_id.clone(),
                    host: game.host.clone(),
                    game_id: Some(session.game_id.clone()),
                    title: Some(game.title.clone()),
                    phase: None,
                };
                let mut entries = without_sessions(&loaded.content.entries);
                let mut entry = session_entry(&active, &mut entries, &loaded.content.entries);
                if let Entry::NowPlaying {
                    local_catalog_game, ..
                } = &mut entry
                {
                    *local_catalog_game = Some(game.clone());
                }
                entries.insert(0, entry);
                loaded.content.entries = entries;
                LaunchablesState::Loaded(loaded)
            }
            other => other,
        }
    }

    /// Only authoritative observations replace the last known session.
    pub fn with_session_status(self, status: &SessionStatusOutcome) -> LaunchablesState {
        let LaunchablesState::Loaded(mut loaded) = self else {
            return LaunchablesState::Loading;
        };
        if let SessionStatusOutcome::Err(failure) = status {
            // A returned far launch is terminal, not an unavailable read.
            // Local startup failures keep their existing exact-cleanup flow.
            if failure.code == "HostLaunchFailed"
                && matches!(
                    loaded.phase,
                    Phase::Ready | Phase::Launching { .. } | Phase::Recovery
                )
                && has_far_session(&loaded.content.entries)
            {
                return retire_failed_far_session(loaded, failure);
            }
        }
        if let SessionStatusOutcome::Ok(payload) = status {
            update_stream_play(&mut loaded.content, payload);
        }
        let active = match status {
            SessionStatusOutcome::Ok(payload) => payload.active.as_ref(),
            SessionStatusOutcome::Err(_) => None,
        };
        let authoritative = is_authoritative_session_status(status);
        let pending_launches = match status {
            SessionStatusOutcome::Ok(payload) => payload.pending_launches.as_deref().unwrap_or(&[]),
            SessionStatusOutcome::Err(_) => &[],
        };
        match loaded.phase {
            Phase::Choosing { choices } => {
                if !authoritative {
                    return match status_failure(status) {
                        Some(failure) => {
                            loaded.content.notice =
                                Some(LaunchNotice::launch(failure.message.clone()));
                            loaded.content.unavailable_session_status = Some(status.clone());
                            // Failed cleanup authorizes exact retry.
                            let choices = if failure.code == "HostRecoveryBlocked" {
                                choices
                                    .into_iter()
                                    .map(|mut choice| {
                                        if choice.cancel == CancelState::Sent {
                                            choice.cancel = CancelState::Retry;
                                        }
                                        choice
                                    })
                                    .collect()
                            } else {
                                choices
                            };
                            LaunchablesState::Loaded(Loaded {
                                phase: Phase::Choosing { choices },
                                content: loaded.content,
                            })
                        }
                        None => LaunchablesState::Loaded(Loaded {
                            phase: Phase::Choosing { choices },
                            content: loaded.content,
                        }),
                    };
                }
                let mut entries = without_sessions(&loaded.content.entries);
                if let Some(active) = active {
                    let entry = session_entry(active, &mut entries, &loaded.content.entries);
                    entries.insert(0, entry);
                }
                let notice = if loaded.content.unavailable_session_status.is_none() {
                    loaded.content.notice
                } else {
                    None
                };
                Self::recover_startup(
                    Content::with_entries(entries, notice),
                    Some(status),
                    &choices,
                )
            }
            Phase::Starting {
                title,
                subject,
                launch_id,
                cancelling,
                cancel_retry_available,
            } => {
                let Some(owned) = launch_id.clone() else {
                    return LaunchablesState::Loaded(Loaded {
                        phase: Phase::Starting {
                            title,
                            subject,
                            launch_id,
                            cancelling,
                            cancel_retry_available,
                        },
                        content: loaded.content,
                    });
                };
                let restore = |content: Content| {
                    LaunchablesState::Loaded(Loaded {
                        phase: Phase::Starting {
                            title: title.clone(),
                            subject: subject.clone(),
                            launch_id: launch_id.clone(),
                            cancelling,
                            cancel_retry_available,
                        },
                        content,
                    })
                };
                let pending = pending_launches
                    .iter()
                    .any(|pending| pending.session.launch_id == owned);
                if !authoritative {
                    let failure = status_failure(status);
                    return match failure {
                        Some(failure)
                            if failure.code == "HostRecoveryBlocked"
                                || failure.code == "HostLaunchFailed" =>
                        {
                            let message = failure.message.clone();
                            restore(loaded.content).startup_problem(&message)
                        }
                        _ => restore(loaded.content),
                    };
                }
                if let Some(active) = active.filter(|active| active.launch_id == owned) {
                    let mut entries = without_sessions(&loaded.content.entries);
                    let entry = session_entry(active, &mut entries, &loaded.content.entries);
                    entries.insert(0, entry);
                    let returned =
                        matches!(active.phase.as_deref(), Some("frozen" | "focus-failed"));
                    let handed_off = matches!(
                        active.initial_handoff,
                        Some(InitialHandoff::Observed | InitialHandoff::Recovered)
                    );
                    return if !cancelling && (returned || handed_off) {
                        ready(Content::with_entries(entries, loaded.content.notice))
                    } else {
                        loaded.content.entries = entries;
                        restore(loaded.content)
                    };
                }
                if pending {
                    return restore(loaded.content);
                }
                ready(Content::with_entries(
                    loaded.content.entries,
                    loaded.content.notice,
                ))
                .with_session_status(status)
            }
            phase => {
                if !authoritative {
                    return match phase {
                        Phase::Ready => {
                            if loaded.content.entries.iter().any(|entry| {
                                entry.session().is_some_and(|session| {
                                    is_local_catalog_session(session, &loaded.content.entries)
                                })
                            }) {
                                loaded.content.unavailable_session_status = Some(status.clone());
                                ready(loaded.content)
                            } else {
                                Self::recover_startup(loaded.content, Some(status), &[])
                            }
                        }
                        phase => LaunchablesState::Loaded(Loaded {
                            phase,
                            content: loaded.content,
                        }),
                    };
                }
                if matches!(phase, Phase::Ready)
                    && loaded.content.unavailable_session_status.is_some()
                {
                    loaded.content.unavailable_session_status = None;
                    return ready(loaded.content).with_session_status(status);
                }
                if let Phase::Launching {
                    return_launch_id: Some(return_launch_id),
                    ..
                } = &phase
                {
                    if active.map(|active| &active.launch_id) != Some(return_launch_id) {
                        return ready(Content::with_entries(
                            loaded.content.entries,
                            loaded.content.notice,
                        ))
                        .with_session_status(status);
                    }
                }
                if matches!(phase, Phase::Recovery) {
                    let mut entries = without_sessions(&loaded.content.entries);
                    if let Some(active) = active {
                        let entry = session_entry(active, &mut entries, &loaded.content.entries);
                        entries.insert(0, entry);
                    }
                    return Self::recover_startup(
                        Content::with_entries(entries, None),
                        Some(status),
                        &[],
                    );
                }
                if !matches!(phase, Phase::Ready) {
                    return LaunchablesState::Loaded(Loaded {
                        phase,
                        content: loaded.content,
                    });
                }
                let previous = loaded
                    .content
                    .entries
                    .iter()
                    .find_map(Entry::session)
                    .cloned();
                if active.is_none() && previous.is_none() {
                    return Self::recover_startup(loaded.content, Some(status), &[]);
                }
                if let (Some(active), Some(previous)) = (active, &previous) {
                    if active.launch_id == previous.launch_id
                        && active.host == previous.host
                        && active.game_id == previous.game_id
                        && active.title == previous.title
                        && active.phase == previous.phase
                        && active.focus_ownership == previous.focus_ownership
                        && active.initial_handoff == previous.initial_handoff
                        && active.steps == previous.steps
                        && active.stream_players == previous.stream_players
                        && !pending_launches
                            .iter()
                            .any(|pending| pending.session.launch_id != active.launch_id)
                    {
                        return ready(loaded.content);
                    }
                }
                let mut entries = without_sessions(&loaded.content.entries);
                if let Some(active) = active {
                    let entry = session_entry(active, &mut entries, &loaded.content.entries);
                    entries.insert(0, entry);
                }
                loaded.content.entries = entries;
                let previous_launch = previous.as_ref().map(|session| &session.launch_id);
                if active.is_some_and(|active| active.phase.as_deref() == Some("stopping"))
                    || active.map(|active| &active.launch_id) != previous_launch
                    || pending_launches
                        .iter()
                        .any(|pending| Some(&pending.session.launch_id) != previous_launch)
                {
                    Self::recover_startup(loaded.content, Some(status), &[])
                } else {
                    ready(loaded.content)
                }
            }
        }
    }

    /// Lock input before the asynchronous stop request leaves.
    pub fn begin_stopping(self, target: &Entry) -> LaunchablesState {
        match (self, target) {
            (LaunchablesState::Loaded(mut loaded), Entry::NowPlaying { session, .. })
                if matches!(loaded.phase, Phase::Ready) =>
            {
                loaded.content.notice = None;
                loaded.phase = Phase::Stopping {
                    launch_id: session.launch_id.clone(),
                };
                LaunchablesState::Loaded(loaded)
            }
            (other, _) => other,
        }
    }

    /// A successful stop request is not the same as an ended session.
    pub fn with_stop_outcome(self, outcome: &SessionEndOutcome) -> LaunchablesState {
        match (self, outcome) {
            (LaunchablesState::Loaded(loaded), SessionEndOutcome::Err(failure))
                if matches!(loaded.phase, Phase::Stopping { .. }) =>
            {
                ready_from(loaded, format!("{}: {}", failure.code, failure.message))
            }
            (other, _) => other,
        }
    }

    /// Fold a stop poll; completion, idle, or a different launch ends it.
    pub fn with_status_after_stop(self, status: &SessionStatusOutcome) -> LaunchablesState {
        let LaunchablesState::Loaded(mut loaded) = self else {
            return LaunchablesState::Loading;
        };
        let Phase::Stopping { launch_id } = &loaded.phase else {
            return LaunchablesState::Loaded(loaded);
        };
        if !is_authoritative_session_status(status) {
            return match status_failure(status).cloned() {
                Some(failure) => {
                    loaded.content.unavailable_session_status = Some(status.clone());
                    ready_from(loaded, format!("{}: {}", failure.code, failure.message))
                }
                None => LaunchablesState::Loaded(loaded),
            };
        }
        if let SessionStatusOutcome::Ok(payload) = status {
            if payload
                .active
                .as_ref()
                .is_some_and(|active| &active.launch_id == launch_id)
            {
                return LaunchablesState::Loaded(loaded);
            }
        }
        ready(Content::with_entries(
            without_sessions(&loaded.content.entries),
            None,
        ))
    }

    pub fn stop_timed_out(self) -> LaunchablesState {
        match self {
            LaunchablesState::Loaded(loaded) if matches!(loaded.phase, Phase::Stopping { .. }) => {
                ready_from(loaded, "The session is still ending.")
            }
            other => other,
        }
    }
}
