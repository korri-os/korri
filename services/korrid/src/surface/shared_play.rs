//! Port of the pure parts of `clients/portal/src/surface/shared-play.ts`:
//! Paused and Ended of shared play this device takes part in, and the
//! invites another device of the same person offers.

use korri_surface_treaty::{SurfaceAction, SurfaceInvite, SurfaceStatus};

use super::launchables::state::{Entry, LaunchablesState, Loaded, Phase};
use crate::{
    invites::Invite,
    netplay::InviteNetplay,
    stream_play::{StreamPlayEndCause, StreamPlayEnded},
};

pub const STREAM: &str = "invite:stream:";
pub const NETPLAY: &str = "invite:netplay:";
pub const DISMISS: &str = "invite:dismiss:";
pub const LEAVE: &str = "stream-play:leave:";

/// The presented entry that holds a copy of `game_id`. Cover art was never
/// published for these (the portal's production client had none).
fn game_entry_id(state: &LaunchablesState, game_id: Option<&str>) -> Option<String> {
    let game_id = game_id?;
    state.entries().iter().find_map(|entry| {
        let own = match entry {
            Entry::NowPlaying { .. } => return None,
            Entry::LocalGame { game, .. } => game.id.as_str(),
            Entry::Game { game, .. } => game.id.as_str(),
        };
        (own == game_id || entry.alternatives().iter().any(|copy| copy.id() == game_id))
            .then(|| entry.key())
    })
}

pub fn stream_end_reason(ended: &StreamPlayEnded) -> String {
    match ended.cause {
        StreamPlayEndCause::DeviceUnreachable => format!("Lost {}", ended.device_label),
        StreamPlayEndCause::SessionEnded => format!(
            "{} ended on {}",
            ended.game_title.as_deref().unwrap_or("The session"),
            ended.device_label
        ),
    }
}

/// Paused and Ended of shared play this device takes part in. Ended covers
/// both kinds: korrid reports a netplay session that ended as it reports a
/// stream that ended, so the words name neither.
pub fn stream_status_from(state: &LaunchablesState) -> Option<SurfaceStatus> {
    let LaunchablesState::Loaded(Loaded {
        phase: Phase::Ready,
        content,
    }) = state
    else {
        return None;
    };
    let active = content.entries.iter().find_map(|entry| {
        entry
            .session()
            .filter(|session| Some(&session.launch_id) == content.stream_play_launch_id.as_ref())
    });
    if let Some(session) = active.filter(|session| session.phase.as_deref() == Some("frozen")) {
        return Some(SurfaceStatus::Paused {
            kicker: format!(
                "Paused on {}",
                session.host.as_deref().unwrap_or("the other device")
            ),
            detail: None,
            actions: vec![SurfaceAction::new(
                format!("{LEAVE}{}", session.launch_id),
                "Leave",
                true,
            )],
            game_id: game_entry_id(state, session.game_id.as_deref()),
        });
    }
    let ended = content.stream_play_ended.as_ref()?;
    Some(SurfaceStatus::Ended {
        kicker: "Session ended".into(),
        reason: stream_end_reason(ended),
        game_id: game_entry_id(state, ended.game_id.as_deref()),
        game_title: ended.game_title.clone(),
    })
}

/// "4.7 GB", "512 MB": a size as a person reads it.
pub fn format_bytes(bytes: f64) -> String {
    let scale = |value: f64, unit: &str| {
        let text = if value < 10.0 {
            format!("{value:.1}")
        } else {
            format!("{}", value.round())
        };
        let text = text.strip_suffix(".0").unwrap_or(&text).to_owned();
        format!("{text} {unit}")
    };
    if bytes >= 1e9 {
        scale(bytes / 1e9, "GB")
    } else if bytes >= 1e6 {
        scale(bytes / 1e6, "MB")
    } else if bytes >= 1e3 {
        scale(bytes / 1e3, "KB")
    } else {
        format!("{} B", bytes.round())
    }
}

/// The words of the netplay route korrid offers for an invite, or nothing.
pub fn netplay_label(netplay: Option<&InviteNetplay>) -> Option<String> {
    match netplay? {
        InviteNetplay::Ready => Some("Play here".into()),
        InviteNetplay::NeedsCopy { bytes } => {
            Some(format!("Copy and play here ({})", format_bytes(*bytes)))
        }
        InviteNetplay::NeedsPlugin { title, .. } => Some(format!("Install {title} and play here")),
        InviteNetplay::Unavailable { .. } => None,
    }
}

pub fn surface_invites_from(invites: &[Invite], _state: &LaunchablesState) -> Vec<SurfaceInvite> {
    invites
        .iter()
        .map(|invite| {
            let netplay = netplay_label(invite.netplay.as_ref());
            let mut actions = vec![SurfaceAction::new(
                format!("{STREAM}{}", invite.invite_id),
                "Play on stream",
                true,
            )];
            if let Some(label) = netplay {
                actions.push(SurfaceAction::new(
                    format!("{NETPLAY}{}", invite.invite_id),
                    label,
                    true,
                ));
            }
            actions.push(SurfaceAction::new(
                format!("{DISMISS}{}", invite.invite_id),
                "Dismiss",
                true,
            ));
            SurfaceInvite {
                id: invite.invite_id.clone(),
                person_name: invite.person_name.clone(),
                device_label: invite.device_label.clone(),
                game_title: invite
                    .session
                    .title
                    .clone()
                    .or_else(|| invite.session.game_id.clone())
                    .unwrap_or_else(|| "Game".into()),
                cover_art_url: None,
                actions,
            }
        })
        .collect()
}
