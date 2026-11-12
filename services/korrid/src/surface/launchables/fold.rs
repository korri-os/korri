//! Port of `clients/portal/src/launchables/fold-games.ts`: legacy's catalog
//! fold. Games only fold when a host supplied one unambiguous release
//! identity. The local copy wins; remote-only groups use stable host/id
//! ordering.

use std::{cmp::Ordering, collections::HashMap};

use crate::{launcher::LocalGame, Game, GameIdentity, PlayStats};

#[derive(Clone, Debug)]
pub enum GameCopy {
    Local(LocalGame),
    Remote(Game),
}

impl GameCopy {
    pub fn id(&self) -> &str {
        match self {
            Self::Local(game) => &game.id,
            Self::Remote(game) => &game.id,
        }
    }
    pub fn identity(&self) -> Option<&GameIdentity> {
        match self {
            Self::Local(game) => game.identity.as_ref(),
            Self::Remote(game) => game.identity.as_ref(),
        }
    }
    pub fn play_stats(&self) -> Option<&PlayStats> {
        match self {
            Self::Local(game) => game.play_stats.as_ref(),
            Self::Remote(game) => game.play_stats.as_ref(),
        }
    }
    pub fn remote(&self) -> Option<&Game> {
        match self {
            Self::Remote(game) => Some(game),
            Self::Local(_) => None,
        }
    }
}

pub struct FoldedGame {
    pub primary: GameCopy,
    pub alternatives: Vec<GameCopy>,
}

fn identity_key(identity: Option<&GameIdentity>) -> Option<String> {
    match identity? {
        GameIdentity::Hash(hash) => Some(format!("hash:{hash}")),
        GameIdentity::Provider(provider) => Some(format!(
            "provider:{}\0{}",
            provider.provider, provider.provider_ref
        )),
    }
}

/// JavaScript `localeCompare` for the ids and host labels this compares.
pub fn locale_compare(left: &str, right: &str) -> Ordering {
    left.to_lowercase()
        .cmp(&right.to_lowercase())
        .then_with(|| right.cmp(left))
}

pub fn compare_copies(left: &GameCopy, right: &GameCopy) -> Ordering {
    match (left, right) {
        (GameCopy::Local(left), GameCopy::Local(right)) => locale_compare(&left.id, &right.id),
        (GameCopy::Remote(left), GameCopy::Remote(right)) => locale_compare(
            left.host.as_deref().unwrap_or(""),
            right.host.as_deref().unwrap_or(""),
        )
        .then_with(|| locale_compare(&left.id, &right.id)),
        (GameCopy::Local(_), GameCopy::Remote(_)) => Ordering::Less,
        (GameCopy::Remote(_), GameCopy::Local(_)) => Ordering::Greater,
    }
}

pub fn fold_game_copies(local_games: &[LocalGame], remote_games: &[Game]) -> Vec<FoldedGame> {
    let copies = local_games
        .iter()
        .cloned()
        .map(GameCopy::Local)
        .chain(remote_games.iter().cloned().map(GameCopy::Remote));
    let mut order: Vec<String> = Vec::new();
    let mut groups: HashMap<String, Vec<GameCopy>> = HashMap::new();
    for (index, copy) in copies.enumerate() {
        let key = identity_key(copy.identity()).unwrap_or_else(|| format!("tagless:{index}"));
        if !groups.contains_key(&key) {
            order.push(key.clone());
        }
        groups.entry(key).or_default().push(copy);
    }
    order
        .into_iter()
        .map(|key| {
            let mut group = groups.remove(&key).expect("group exists");
            let primary_index = group
                .iter()
                .position(|copy| matches!(copy, GameCopy::Local(_)))
                .unwrap_or_else(|| {
                    let mut indices: Vec<usize> = (0..group.len()).collect();
                    indices.sort_by(|left, right| compare_copies(&group[*left], &group[*right]));
                    indices[0]
                });
            let primary = group.remove(primary_index);
            group.sort_by(compare_copies);
            FoldedGame {
                primary,
                alternatives: group,
            }
        })
        .collect()
}

/// Play stats for one folded game. Play history is game-centric, not
/// host-centric: the newest lastPlayed across every copy wins, counts and
/// playtime sum. None when no copy has been played.
pub fn merge_play_stats<'a>(copies: impl IntoIterator<Item = &'a GameCopy>) -> Option<PlayStats> {
    let played: Vec<&PlayStats> = copies
        .into_iter()
        .filter_map(GameCopy::play_stats)
        .collect();
    match played.len() {
        0 => None,
        1 => Some(played[0].clone()),
        _ => {
            let mut last_played: Option<String> = None;
            let mut play_count = 0;
            let mut total_playtime_seconds = 0.0;
            for stats in played {
                if let Some(candidate) = &stats.last_played {
                    if last_played.as_ref().is_none_or(|last| candidate > last) {
                        last_played = Some(candidate.clone());
                    }
                }
                play_count += stats.play_count;
                total_playtime_seconds += stats.total_playtime_seconds;
            }
            Some(PlayStats {
                last_played,
                play_count,
                total_playtime_seconds,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    //! Port of `fold-games.test.ts`.
    use super::*;
    use crate::{GameProviderIdentity, GameSource};

    pub fn remote(id: &str, host: Option<&str>, identity: Option<GameIdentity>) -> Game {
        Game {
            id: id.into(),
            title: id.into(),
            host: host.map(Into::into),
            identity,
            source: GameSource {
                device_public_key: None,
                label: host.unwrap_or("This device").into(),
                is_local: false,
            },
            supports_runner_selection: false,
            play_stats: None,
        }
    }
    pub fn local(id: &str, identity: Option<GameIdentity>) -> LocalGame {
        LocalGame {
            id: id.into(),
            title: id.into(),
            system: "GBA".into(),
            system_id: Some("gba".into()),
            identity,
            cover_asset_id: None,
            play_stats: None,
        }
    }
    fn hash(value: &str) -> Option<GameIdentity> {
        Some(GameIdentity::Hash(value.into()))
    }

    #[test]
    fn folds_only_shared_identities_and_prefers_the_local_copy() {
        let folded = fold_game_copies(
            &[local("l1", hash("a")), local("l2", None)],
            &[
                remote("r2", Some("zao"), hash("a")),
                remote("r1", Some("aka"), hash("a")),
                remote("r3", Some("zao"), None),
            ],
        );
        assert_eq!(folded.len(), 3);
        assert_eq!(folded[0].primary.id(), "l1");
        assert_eq!(
            folded[0]
                .alternatives
                .iter()
                .map(GameCopy::id)
                .collect::<Vec<_>>(),
            vec!["r1", "r2"]
        );
        assert_eq!(folded[1].primary.id(), "l2");
        assert_eq!(folded[2].primary.id(), "r3");
    }

    #[test]
    fn remote_only_groups_take_the_first_host_in_order() {
        let provider = Some(GameIdentity::Provider(GameProviderIdentity {
            provider: "bbs".into(),
            provider_ref: "1".into(),
        }));
        let folded = fold_game_copies(
            &[],
            &[
                remote("b", Some("zao"), provider.clone()),
                remote("a", Some("aka"), provider),
            ],
        );
        assert_eq!(folded.len(), 1);
        assert_eq!(folded[0].primary.id(), "a");
        assert_eq!(folded[0].alternatives[0].id(), "b");
    }

    #[test]
    fn merges_play_stats_across_copies() {
        let mut first = remote("a", Some("zao"), None);
        first.play_stats = Some(PlayStats {
            last_played: Some("2026-01-02T00:00:00Z".into()),
            play_count: 2,
            total_playtime_seconds: 10.0,
        });
        let mut second = remote("b", Some("aka"), None);
        second.play_stats = Some(PlayStats {
            last_played: Some("2026-03-01T00:00:00Z".into()),
            play_count: 1,
            total_playtime_seconds: 5.0,
        });
        let third = remote("c", Some("aka"), None);
        let copies = [
            GameCopy::Remote(first),
            GameCopy::Remote(second),
            GameCopy::Remote(third.clone()),
        ];
        let merged = merge_play_stats(&copies).unwrap();
        assert_eq!(merged.last_played.as_deref(), Some("2026-03-01T00:00:00Z"));
        assert_eq!(merged.play_count, 3);
        assert_eq!(merged.total_playtime_seconds, 15.0);
        assert!(merge_play_stats(&[GameCopy::Remote(third)]).is_none());
    }
}
