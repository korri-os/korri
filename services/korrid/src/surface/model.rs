//! Port of `clients/portal/src/surface/surface-model.ts`: Korri's launchables
//! state, expressed in the surface treaty. Everything a surface is allowed to
//! know is decided here, in pure functions, so no surface ever sees a korrid
//! outcome or an error code. Anything Korri cannot honestly report is absent.

use korri_surface_treaty::{
    SurfaceAction, SurfaceGame, SurfaceGameDownload, SurfaceLaunchLocation, SurfaceStatus,
    SurfaceTrue,
};

use super::{
    downloads::GameDownloadFacts,
    launchables::{
        fold::{compare_copies, locale_compare, merge_play_stats, GameCopy},
        state::{
            is_local_catalog_session, Entry, LaunchNotice, LaunchSubject, LaunchablesState, Loaded,
            Phase,
        },
    },
    shared_play::stream_status_from,
};
use crate::SessionStatusOutcome;

/// Section captions. Grouping is Korri's call; the surface only renders it.
const SECTION_CONTINUE: &str = "Continue";
const SECTION_THIS_DEVICE: &str = "This device";

fn primary_copy(entry: &Entry) -> Option<GameCopy> {
    match entry {
        Entry::LocalGame { game, .. } => Some(GameCopy::Local(game.clone())),
        Entry::Game { game, .. } => Some(GameCopy::Remote(game.clone())),
        Entry::NowPlaying { .. } => None,
    }
}

fn copy_is_local(copy: &GameCopy) -> bool {
    match copy {
        GameCopy::Local(_) => true,
        GameCopy::Remote(game) => game.source.is_local,
    }
}

pub fn ordered_copies_for_entry(entry: &Entry) -> Vec<GameCopy> {
    let Some(primary) = primary_copy(entry) else {
        return Vec::new();
    };
    let mut copies = vec![primary];
    copies.extend(entry.alternatives().iter().cloned());
    copies.sort_by(|left, right| {
        if copy_is_local(left) != copy_is_local(right) {
            return if copy_is_local(left) {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            };
        }
        compare_copies(left, right)
    });
    copies
}

fn copy_host_key(copy: &GameCopy) -> String {
    match copy {
        GameCopy::Remote(game) if !game.source.is_local => {
            format!("remote:{}", game.host.as_deref().unwrap_or(""))
        }
        _ => "local".into(),
    }
}

fn copy_location_label(copy: &GameCopy) -> String {
    match copy {
        GameCopy::Remote(game) if !game.source.is_local => {
            game.host.clone().unwrap_or_else(|| "Other device".into())
        }
        _ => "This device".into(),
    }
}

fn copy_location_id(copy: &GameCopy) -> String {
    let (kind, host) = match copy {
        GameCopy::Local(_) => ("local", None),
        GameCopy::Remote(game) => ("remote", game.host.clone()),
    };
    serde_json::to_string(&(kind, host, copy.id())).expect("location id serializes")
}

fn distinct_host_copies(entry: &Entry) -> Vec<GameCopy> {
    let mut seen = std::collections::HashSet::new();
    ordered_copies_for_entry(entry)
        .into_iter()
        .filter(|copy| seen.insert(copy_host_key(copy)))
        .collect()
}

/// Host choices for a folded game. Local is always first when it exists.
pub fn launch_locations_for_entry(entry: &Entry) -> Vec<SurfaceLaunchLocation> {
    let copies = distinct_host_copies(entry);
    if copies.len() < 2 {
        return Vec::new();
    }
    copies
        .iter()
        .map(|copy| SurfaceLaunchLocation {
            id: copy_location_id(copy),
            label: copy_location_label(copy),
        })
        .collect()
}

/// Resolve an opaque surface choice to one exact copy, with no fallback.
pub fn entry_for_launch_location(entry: &Entry, launch_location_id: &str) -> Option<Entry> {
    let copy = distinct_host_copies(entry)
        .into_iter()
        .find(|candidate| copy_location_id(candidate) == launch_location_id)?;
    // A local record supplies display metadata, not a launch route.
    let launchable = match &copy {
        GameCopy::Local(_) => ordered_copies_for_entry(entry)
            .into_iter()
            .find(|candidate| matches!(candidate, GameCopy::Remote(game) if game.source.is_local))
            .unwrap_or(copy),
        GameCopy::Remote(_) => copy,
    };
    Some(Entry::for_copy(&launchable))
}

/// Play facts for a game entry. Parses the UTC ISO string once here.
fn play_facts_for_entry(entry: &Entry, game: &mut SurfaceGame) {
    if entry.is_now_playing() {
        return;
    }
    let Some(stats) = merge_play_stats(&ordered_copies_for_entry(entry)) else {
        return;
    };
    let Some(last_played) = &stats.last_played else {
        return;
    };
    let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(last_played) else {
        return;
    };
    game.last_played_at = Some(parsed.timestamp_millis() as f64);
    game.play_count = Some(stats.play_count);
    game.total_playtime_seconds = Some(stats.total_playtime_seconds);
}

fn alternative_location(entry: &Entry) -> Option<String> {
    let primary_key = copy_host_key(&primary_copy(entry)?);
    let visible: Vec<String> = distinct_host_copies(entry)
        .iter()
        .filter(|copy| copy_host_key(copy) != primary_key)
        .map(|copy| {
            if copy_is_local(copy) {
                "this device".into()
            } else {
                copy_location_label(copy)
            }
        })
        .collect();
    (!visible.is_empty()).then(|| format!("Also on {}", visible.join(", ")))
}

fn bare_game(id: String, title: String) -> SurfaceGame {
    SurfaceGame {
        id,
        title,
        launch_locations: None,
        section: None,
        subtitle: None,
        cover_art_url: None,
        resumable: None,
        active_profile: None,
        last_played_at: None,
        play_count: None,
        total_playtime_seconds: None,
        saves: None,
        download: None,
    }
}

pub fn game_from_entry(entry: &Entry, entries: &[Entry]) -> SurfaceGame {
    match entry {
        Entry::NowPlaying { session, .. } => {
            let mut game = bare_game(
                entry.key(),
                session
                    .title
                    .clone()
                    .or_else(|| session.game_id.clone())
                    .unwrap_or_else(|| "Current session".into()),
            );
            game.section = Some(SECTION_CONTINUE.into());
            game.subtitle = Some(if is_local_catalog_session(session, entries) {
                SECTION_THIS_DEVICE.into()
            } else {
                session.host.clone().unwrap_or_else(|| "Running now".into())
            });
            game.resumable = Some(true);
            game
        }
        Entry::LocalGame { game: local, .. } => {
            let mut game = bare_game(entry.key(), local.title.clone());
            game.section = Some(SECTION_THIS_DEVICE.into());
            game.subtitle = Some(match alternative_location(entry) {
                None => local.system.clone(),
                Some(alternative) => format!("{} · {alternative}", local.system),
            });
            // The portal's production client published no local cover art.
            let locations = launch_locations_for_entry(entry);
            if locations.len() >= 2 {
                game.launch_locations = Some(locations);
            }
            play_facts_for_entry(entry, &mut game);
            game
        }
        Entry::Game { game: remote, .. } => {
            let mut game = bare_game(entry.key(), remote.title.clone());
            let location = if remote.source.is_local {
                Some(SECTION_THIS_DEVICE.to_owned())
            } else {
                remote.host.clone()
            };
            let subtitle: Vec<String> = [location.clone(), alternative_location(entry)]
                .into_iter()
                .flatten()
                .filter(|part| !part.is_empty())
                .collect();
            game.section = Some(location.unwrap_or_else(|| "Other devices".into()));
            if !subtitle.is_empty() {
                game.subtitle = Some(subtitle.join(" · "));
            }
            let locations = launch_locations_for_entry(entry);
            if locations.len() >= 2 {
                game.launch_locations = Some(locations);
            }
            play_facts_for_entry(entry, &mut game);
            game
        }
    }
}

/// The game action ids that act on a game's download (ADR 0013).
pub const DOWNLOAD_ACTION_IDS: [&str; 3] = ["download", "cancel-download", "remove-download"];

/// The treaty's `download` for a game, from what korrid's artifacts add up to.
pub fn surface_download_from(facts: Option<&GameDownloadFacts>) -> Option<SurfaceGameDownload> {
    let facts = facts?;
    if !facts.needs {
        return None;
    }
    Some(SurfaceGameDownload {
        queued: facts.queued.then_some(SurfaceTrue),
        progress: facts.progress.clone(),
        bytes: facts.bytes,
    })
}

/// What the download actions of one game are decided from.
#[derive(Clone, Debug)]
pub struct GameDownloadContext {
    pub facts: GameDownloadFacts,
    /// A session of this game is live, here or anywhere korrid reports it.
    pub live: bool,
    /// Korri can take a game action now.
    pub enabled: bool,
}

fn download_actions(context: &GameDownloadContext) -> Vec<SurfaceAction> {
    let GameDownloadContext {
        facts,
        live,
        enabled,
    } = context;
    let mut actions = Vec::new();
    if facts.needs && !facts.active {
        let mut action = SurfaceAction::new("download", "Download", *enabled);
        if let Some(failure) = &facts.failure {
            action = action.with_description(format!("The last try failed. {failure}"));
        }
        actions.push(action);
    }
    if facts.active {
        let mut action = SurfaceAction::new(
            "cancel-download",
            "Cancel download",
            *enabled && facts.stoppable,
        );
        if !facts.stoppable {
            action = action.with_description(if facts.shared {
                "Another game's download needs what is left, so it goes on."
            } else {
                "Unpacking cannot be stopped. It ends on its own."
            });
        }
        actions.push(action);
    }
    if facts.own_ready && !facts.active && !live {
        actions.push(
            SurfaceAction::new("remove-download", "Remove download", *enabled)
                .destructive()
                .with_description(
                    "Deletes its downloaded files from this device. Your saves stay.",
                ),
        );
    }
    actions
}

/// The running session can be continued or stopped. A local game whose files
/// or runtime come from artifacts can be downloaded, cancelled and removed.
pub fn game_actions_for_entry(
    entry: Option<&Entry>,
    download: Option<&GameDownloadContext>,
) -> Vec<SurfaceAction> {
    if matches!(entry, Some(Entry::NowPlaying { .. })) {
        return vec![
            SurfaceAction::new("resume", "Resume", true),
            SurfaceAction::new("end", "End", true).destructive(),
        ];
    }
    download.map(download_actions).unwrap_or_default()
}

/// Surface status names an actual presented entry, never a raw Rust game id.
fn status_game_id(loaded: &Loaded, subject: Option<&LaunchSubject>) -> Option<String> {
    let subject = subject?;
    let entries = &loaded.content.entries;
    if let Some(entry_id) = &subject.entry_id {
        return entries
            .iter()
            .any(|entry| &entry.key() == entry_id)
            .then(|| entry_id.clone());
    }
    let id = subject.id.as_ref()?;
    let (local, launch_id) = match &loaded.phase {
        Phase::Starting { launch_id, .. } => (true, launch_id.clone()),
        Phase::Launching {
            return_launch_id: Some(launch_id),
            ..
        } => (true, Some(launch_id.clone())),
        _ => (false, None),
    };
    let matches = |copy: &GameCopy| {
        copy.id() == id
            && (!local || matches!(copy, GameCopy::Remote(game) if game.source.is_local))
    };
    let games: Vec<&Entry> = entries
        .iter()
        .filter(|entry| {
            !entry.is_now_playing() && ordered_copies_for_entry(entry).iter().any(&matches)
        })
        .collect();
    match games.len() {
        1 => return Some(games[0].key()),
        0 => {}
        _ => return None,
    }
    entries
        .iter()
        .find(|entry| {
            entry.session().is_some_and(|session| {
                Some(&session.launch_id) == launch_id.as_ref()
                    && session.game_id.as_ref() == Some(id)
            })
        })
        .map(Entry::key)
}

fn busy(kicker: String, detail: Option<String>, game_id: Option<String>) -> SurfaceStatus {
    SurfaceStatus::Busy {
        kicker,
        detail,
        progress: None,
        actions: None,
        decline_action_id: None,
        game_id,
    }
}

fn problem(kicker: impl Into<String>, reason: impl Into<String>, can_retry: bool) -> SurfaceStatus {
    SurfaceStatus::Problem {
        kicker: kicker.into(),
        reason: reason.into(),
        can_retry,
        game_id: None,
        game_title: None,
    }
}

pub fn status_from(state: &LaunchablesState) -> SurfaceStatus {
    let LaunchablesState::Loaded(loaded) = state else {
        return SurfaceStatus::Browsing;
    };
    let subject = match &loaded.phase {
        Phase::Preparing { subject, .. } | Phase::Launching { subject, .. } => subject.clone(),
        Phase::Starting { subject, .. } => Some(subject.clone()),
        Phase::Ready => match &loaded.content.notice {
            Some(LaunchNotice::Launch { subject, .. }) => subject.clone(),
            _ => None,
        },
        _ => None,
    };
    let game_id = status_game_id(loaded, subject.as_ref());
    let notice_message = loaded
        .content
        .notice
        .as_ref()
        .map(|notice| notice.message().to_owned());
    match &loaded.phase {
        Phase::Preparing { title, .. } => busy(
            format!("Preparing {title}…"),
            Some("Opening your session".into()),
            game_id,
        ),
        Phase::Launching {
            return_launch_id,
            title,
            ..
        } => busy(
            if return_launch_id.is_none() {
                format!("Starting {title}…")
            } else {
                format!("Returning to {title}…")
            },
            Some("Opening your session".into()),
            game_id,
        ),
        Phase::Choosing { choices } => SurfaceStatus::Busy {
            kicker: format!("{} launches are starting", choices.len()),
            detail: Some(
                notice_message.unwrap_or_else(|| "Cancel each launch you do not want.".into()),
            ),
            progress: None,
            actions: Some(
                choices
                    .iter()
                    .enumerate()
                    .map(|(index, choice)| {
                        use super::launchables::state::{CancelState, ChoicePhase};
                        use crate::PendingLaunchPhase;
                        let same_title = choices
                            .iter()
                            .filter(|candidate| candidate.title == choice.title)
                            .count();
                        let number = choices[..=index]
                            .iter()
                            .filter(|candidate| candidate.title == choice.title)
                            .count();
                        let description = if choice.cancel == CancelState::Retry {
                            "Cancel failed. Try again."
                        } else {
                            match choice.phase {
                                ChoicePhase::Pending(PendingLaunchPhase::Reserved) => {
                                    "Waiting to start"
                                }
                                ChoicePhase::Pending(PendingLaunchPhase::Preparing) => "Preparing",
                                ChoicePhase::Pending(PendingLaunchPhase::Committing) => "Starting",
                                ChoicePhase::Waiting => "Waiting for its window",
                                ChoicePhase::Pending(PendingLaunchPhase::Cancelling) => {
                                    "Cancelling"
                                }
                            }
                        };
                        SurfaceAction::new(
                            format!("cancel-pending:{}", choice.launch_id),
                            format!(
                                "Cancel {}{}",
                                choice.title,
                                if same_title > 1 {
                                    format!(" ({number})")
                                } else {
                                    String::new()
                                }
                            ),
                            choice.cancel == CancelState::Retry
                                || (choice.cancel == CancelState::Idle
                                    && choice.phase
                                        != ChoicePhase::Pending(PendingLaunchPhase::Cancelling)),
                        )
                        .with_description(description)
                    })
                    .collect(),
            ),
            decline_action_id: None,
            game_id: None,
        },
        Phase::Starting {
            title,
            cancelling,
            cancel_retry_available,
            ..
        } => SurfaceStatus::Busy {
            kicker: if *cancelling {
                "Cancelling launch…".into()
            } else {
                format!("Starting {title}…")
            },
            detail: Some(notice_message.unwrap_or_else(|| {
                if *cancelling {
                    "Waiting for the exact launch to end".into()
                } else {
                    "Opening your session".into()
                }
            })),
            progress: None,
            actions: Some(vec![SurfaceAction::new(
                "cancel-launch",
                if *cancelling && *cancel_retry_available == Some(true) {
                    "Retry Cancel"
                } else {
                    "Cancel"
                },
                !*cancelling || *cancel_retry_available == Some(true),
            )]),
            decline_action_id: None,
            game_id,
        },
        Phase::Stopping { .. } => busy(
            "Ending session…".into(),
            Some("Waiting for the host to finish".into()),
            None,
        ),
        Phase::Recovery => problem(
            "Launch recovery needed",
            notice_message.unwrap_or_default(),
            true,
        ),
        Phase::Ready => {
            if let Some(shared) = stream_status_from(state) {
                return shared;
            }
            let failure = match &loaded.content.unavailable_session_status {
                Some(SessionStatusOutcome::Err(failure)) => Some(failure),
                Some(SessionStatusOutcome::Ok(payload)) => payload.observation_failure.as_ref(),
                None => None,
            };
            if let Some(failure) = failure {
                let reason: Vec<String> = [
                    notice_message,
                    Some(format!("{}: {}", failure.code, failure.message)),
                ]
                .into_iter()
                .flatten()
                .filter(|part| !part.is_empty())
                .collect();
                return problem("Live session recovery needed", reason.join(" · "), true);
            }
            match &loaded.content.notice {
                None => SurfaceStatus::Browsing,
                Some(notice) => problem_from_notice(notice, game_id),
            }
        }
    }
}

/// `game_id` is the presented entry for the notice's subject.
fn problem_from_notice(notice: &LaunchNotice, game_id: Option<String>) -> SurfaceStatus {
    // Nothing about the failure changed, so an immediate second attempt would
    // fail identically; the user acknowledges instead.
    let reason = notice.message().to_owned();
    match notice {
        LaunchNotice::Catalog { .. } => problem("Catalog problem", reason, false),
        LaunchNotice::Stop { .. } => problem("Couldn't end the session", reason, false),
        LaunchNotice::Action { .. } => problem("Operation failed", reason, false),
        LaunchNotice::Launch { subject, .. } => SurfaceStatus::Problem {
            kicker: match subject {
                Some(subject) => format!("Couldn't start {}", subject.title),
                None => "Couldn't start".into(),
            },
            reason,
            can_retry: false,
            game_id: subject.as_ref().and(game_id),
            game_title: subject.as_ref().map(|subject| subject.title.clone()),
        },
    }
}

/// Every game Korri publishes, in Korri's order. None while Loading.
pub fn catalog_games(state: &LaunchablesState) -> Option<Vec<SurfaceGame>> {
    let loaded = state.loaded()?;
    let entries = &loaded.content.entries;
    Some(
        entries
            .iter()
            .map(|entry| game_from_entry(entry, entries))
            .collect(),
    )
}

/// Find the entry a surface id refers to. Surface ids are entry keys.
pub fn entry_for_id<'a>(state: &'a LaunchablesState, id: &str) -> Option<&'a Entry> {
    state.entries().iter().find(|entry| entry.key() == id)
}

/// Keep `locale_compare` reachable for callers that sort the way the portal did.
pub fn compare_labels(left: &str, right: &str) -> std::cmp::Ordering {
    locale_compare(left, right)
}
