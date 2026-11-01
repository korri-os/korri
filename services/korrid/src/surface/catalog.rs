//! The cursor catalog (native surfaces contract, section 3.4).
//!
//! korrid holds the whole game list in Korri's order. A surface sees one
//! window of it through a query: a title filter, a section and an order.
//! Cursors name the game at a window's edge plus that game's sort key, so a
//! cursor stays valid when games are added elsewhere: korrid finds the game
//! again by id, and falls back to the key when the game itself went away.
//!
//! The orders match Pico's `pico-library-view.ts`: `korri` keeps Korri's
//! order, `title` sorts by title, `mostPlayed` by recorded playtime and
//! `recent` by newest session end. A game without the field sorts last, in
//! Korri's order. Every sort is stable.

use std::{cmp::Ordering, collections::HashMap, sync::Arc};

use korri_surface_treaty::{
    CatalogFrom, CatalogOrder, CatalogQuery, Cursor, SurfaceCatalog, SurfaceGame, CATALOG_MAX_COUNT,
};
use serde::{Deserialize, Serialize};

/// The whole game list in Korri's order, with the lowercase titles every
/// filter and title sort reads. Built once per published list.
pub struct CatalogGames {
    games: Vec<SurfaceGame>,
    lower_titles: Vec<String>,
}

impl CatalogGames {
    pub fn new(games: Vec<SurfaceGame>) -> Self {
        let lower_titles = games.iter().map(|game| game.title.to_lowercase()).collect();
        Self {
            games,
            lower_titles,
        }
    }
    pub fn len(&self) -> usize {
        self.games.len()
    }
    pub fn is_empty(&self) -> bool {
        self.games.is_empty()
    }
    pub fn games(&self) -> &[SurfaceGame] {
        &self.games
    }
    pub fn get(&self, id: &str) -> Option<&SurfaceGame> {
        self.games.iter().find(|game| game.id == id)
    }
}

/// One query over one game list: the matching games in query order, and
/// where each sits. A surface keeps one of these while its query and the
/// list stay the same, so moving a window costs only the window.
pub struct CatalogIndex {
    games: Arc<CatalogGames>,
    query: CatalogQuery,
    order: Vec<u32>,
    positions: HashMap<String, u32>,
}

/// The sort key a cursor carries, so a cursor whose game went away still
/// finds its place.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(untagged)]
enum SortKey {
    /// Korri's own position of the game when the cursor was made.
    Index(u32),
    /// Lowercase title, then the title itself, then Korri's position: the
    /// stable sort breaks ties in Korri's order.
    Title(String, String, u32),
    /// Playtime or last played (absent sorts last), then Korri's position.
    Number(Option<f64>, u32),
}

#[derive(Debug, Deserialize, Serialize)]
struct CursorBody {
    key: SortKey,
    id: String,
}

fn compare_titles(left: (&str, &str), right: (&str, &str)) -> Ordering {
    // Lowercase first, as `localeCompare` puts "a" before "A".
    left.0.cmp(right.0).then_with(|| right.1.cmp(left.1))
}

fn compare_numbers(left: Option<f64>, right: Option<f64>) -> Ordering {
    match (left, right) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(left), Some(right)) => right.partial_cmp(&left).unwrap_or(Ordering::Equal),
    }
}

fn compare_keys(left: &SortKey, right: &SortKey) -> Ordering {
    match (left, right) {
        (SortKey::Index(left), SortKey::Index(right)) => left.cmp(right),
        (
            SortKey::Title(lower, title, index),
            SortKey::Title(other_lower, other_title, other_index),
        ) => compare_titles((lower, title), (other_lower, other_title))
            .then_with(|| index.cmp(other_index)),
        (SortKey::Number(left, index), SortKey::Number(right, other_index)) => {
            compare_numbers(*left, *right).then_with(|| index.cmp(other_index))
        }
        _ => Ordering::Equal,
    }
}

fn number_of(game: &SurfaceGame, order: CatalogOrder) -> Option<f64> {
    match order {
        CatalogOrder::MostPlayed => game.total_playtime_seconds,
        CatalogOrder::Recent => game.last_played_at,
        CatalogOrder::Korri | CatalogOrder::Title => None,
    }
}

impl CatalogIndex {
    pub fn new(games: Arc<CatalogGames>, query: CatalogQuery) -> Self {
        let needle = query.text.trim().to_lowercase();
        let mut order: Vec<u32> = (0..games.games.len() as u32)
            .filter(|index| {
                let game = &games.games[*index as usize];
                query
                    .section
                    .as_ref()
                    .is_none_or(|section| game.section.as_ref() == Some(section))
                    && (needle.is_empty() || games.lower_titles[*index as usize].contains(&needle))
            })
            .collect();
        match query.order {
            CatalogOrder::Korri => {}
            CatalogOrder::Title => order.sort_by(|left, right| {
                compare_titles(
                    (
                        &games.lower_titles[*left as usize],
                        &games.games[*left as usize].title,
                    ),
                    (
                        &games.lower_titles[*right as usize],
                        &games.games[*right as usize].title,
                    ),
                )
            }),
            CatalogOrder::MostPlayed | CatalogOrder::Recent => order.sort_by(|left, right| {
                compare_numbers(
                    number_of(&games.games[*left as usize], query.order),
                    number_of(&games.games[*right as usize], query.order),
                )
            }),
        }
        let positions = order
            .iter()
            .enumerate()
            .map(|(position, index)| (games.games[*index as usize].id.clone(), position as u32))
            .collect();
        Self {
            games,
            query,
            order,
            positions,
        }
    }

    pub fn query(&self) -> &CatalogQuery {
        &self.query
    }
    pub fn games(&self) -> &Arc<CatalogGames> {
        &self.games
    }
    pub fn total(&self) -> u32 {
        self.order.len() as u32
    }

    fn key_at(&self, position: u32) -> SortKey {
        let index = self.order[position as usize] as usize;
        let game = &self.games.games[index];
        match self.query.order {
            CatalogOrder::Korri => SortKey::Index(index as u32),
            CatalogOrder::Title => SortKey::Title(
                self.games.lower_titles[index].clone(),
                game.title.clone(),
                index as u32,
            ),
            CatalogOrder::MostPlayed | CatalogOrder::Recent => {
                SortKey::Number(number_of(game, self.query.order), index as u32)
            }
        }
    }

    fn cursor_at(&self, position: u32) -> Cursor {
        let index = self.order[position as usize] as usize;
        let body = CursorBody {
            key: self.key_at(position),
            id: self.games.games[index].id.clone(),
        };
        Cursor(serde_json::to_string(&body).expect("cursor body serializes"))
    }

    /// The first position whose key compares `Greater` than `key`
    /// (`strict`), or is not `Less` than it.
    fn partition(&self, key: &SortKey, strict: bool) -> u32 {
        binary_partition(self.order.len(), |position| {
            let ordering = compare_keys(&self.key_at(position as u32), key);
            if strict {
                ordering != Ordering::Greater
            } else {
                ordering == Ordering::Less
            }
        }) as u32
    }

    /// Where the window starts and how long it is.
    pub fn resolve(&self, from: &CatalogFrom, count: u16) -> (u32, u32) {
        let count = u32::from(count.clamp(1, CATALOG_MAX_COUNT));
        let total = self.total();
        let start = match from {
            CatalogFrom::Start => 0,
            CatalogFrom::End => total.saturating_sub(count),
            CatalogFrom::After { cursor } => match self.locate(cursor) {
                Located::At(position) => position + 1,
                // Korri's order keeps the removed game's old position: the
                // game that now holds it came after the removed one.
                Located::Key(key) => {
                    let strict = !matches!(key, SortKey::Index(_));
                    self.partition(&key, strict)
                }
                Located::Unknown => 0,
            },
            CatalogFrom::Before { cursor } => {
                let end = match self.locate(cursor) {
                    Located::At(position) => position,
                    Located::Key(key) => self.partition(&key, false),
                    Located::Unknown => count.min(total),
                };
                end.saturating_sub(count)
            }
            CatalogFrom::Around { game_id } => match self.positions.get(game_id) {
                Some(position) => position
                    .saturating_sub(count / 2)
                    .min(total.saturating_sub(count)),
                None => 0,
            },
        };
        let start = start.min(total);
        (start, count.min(total - start))
    }

    /// One window of this query, as the model carries it.
    pub fn window(&self, from: &CatalogFrom, count: u16) -> SurfaceCatalog {
        let (start, length) = self.resolve(from, count);
        let end = start + length;
        SurfaceCatalog::Ready {
            total: self.total(),
            window: self.order[start as usize..end as usize]
                .iter()
                .map(|index| self.games.games[*index as usize].clone())
                .collect(),
            first_index: start,
            before: (start > 0 && length > 0).then(|| self.cursor_at(start)),
            after: (end < self.total() && length > 0).then(|| self.cursor_at(end - 1)),
            query: self.query.clone(),
        }
    }

    fn locate(&self, cursor: &Cursor) -> Located {
        let Ok(body) = serde_json::from_str::<CursorBody>(&cursor.0) else {
            return Located::Unknown;
        };
        if let Some(position) = self.positions.get(&body.id) {
            return Located::At(*position);
        }
        // A cursor from another order has the wrong kind of key.
        let expected = match self.query.order {
            CatalogOrder::Korri => matches!(body.key, SortKey::Index(_)),
            CatalogOrder::Title => matches!(body.key, SortKey::Title(..)),
            CatalogOrder::MostPlayed | CatalogOrder::Recent => {
                matches!(body.key, SortKey::Number(..))
            }
        };
        if !expected {
            return Located::Unknown;
        }
        Located::Key(body.key)
    }
}

enum Located {
    At(u32),
    Key(SortKey),
    Unknown,
}

fn binary_partition(length: usize, mut before: impl FnMut(usize) -> bool) -> usize {
    let (mut low, mut high) = (0, length);
    while low < high {
        let middle = low + (high - low) / 2;
        if before(middle) {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    low
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn game(id: usize) -> SurfaceGame {
        SurfaceGame {
            id: format!("game:{id:06}"),
            title: format!("Title {:06}", (id * 7919) % 100_003),
            launch_locations: None,
            section: Some(
                if id.is_multiple_of(3) {
                    "Continue"
                } else {
                    "This device"
                }
                .into(),
            ),
            subtitle: None,
            cover_art_url: None,
            resumable: None,
            active_profile: None,
            last_played_at: (!id.is_multiple_of(4)).then_some((id * 13 % 1000) as f64),
            play_count: None,
            total_playtime_seconds: (!id.is_multiple_of(5)).then_some((id * 31 % 777) as f64),
            saves: None,
            download: None,
        }
    }

    fn games(count: usize) -> Arc<CatalogGames> {
        Arc::new(CatalogGames::new((0..count).map(game).collect()))
    }

    fn ready(catalog: SurfaceCatalog) -> (u32, Vec<String>, u32, Option<Cursor>, Option<Cursor>) {
        let SurfaceCatalog::Ready {
            total,
            window,
            first_index,
            before,
            after,
            ..
        } = catalog
        else {
            panic!("not ready")
        };
        (
            total,
            window.into_iter().map(|game| game.id).collect(),
            first_index,
            before,
            after,
        )
    }

    fn ids(range: std::ops::Range<usize>) -> Vec<String> {
        range.map(|id| format!("game:{id:06}")).collect()
    }

    #[test]
    fn zero_games_give_an_empty_window_without_cursors() {
        let index = CatalogIndex::new(games(0), CatalogQuery::default());
        for from in [
            CatalogFrom::Start,
            CatalogFrom::End,
            CatalogFrom::Around {
                game_id: "game:000001".into(),
            },
        ] {
            assert_eq!(ready(index.window(&from, 50)), (0, vec![], 0, None, None));
        }
    }

    #[test]
    fn twelve_games_page_forward_and_back() {
        let index = CatalogIndex::new(games(12), CatalogQuery::default());
        let (total, window, first, before, after) = ready(index.window(&CatalogFrom::Start, 5));
        assert_eq!(
            (total, window, first, before.is_none()),
            (12, ids(0..5), 0, true)
        );
        let after = after.unwrap();
        let (_, window, first, before, after) =
            ready(index.window(&CatalogFrom::After { cursor: after }, 5));
        assert_eq!((window, first), (ids(5..10), 5));
        let (_, window, first, _, last_after) = ready(index.window(
            &CatalogFrom::After {
                cursor: after.unwrap(),
            },
            5,
        ));
        assert_eq!((window, first, last_after), (ids(10..12), 10, None));
        let (_, window, first, _, _) = ready(index.window(
            &CatalogFrom::Before {
                cursor: before.unwrap(),
            },
            5,
        ));
        assert_eq!((window, first), (ids(0..5), 0));
        let (_, window, first, _, after) = ready(index.window(&CatalogFrom::End, 5));
        assert_eq!((window, first, after), (ids(7..12), 7, None));
        let (_, window, first, _, _) = ready(index.window(
            &CatalogFrom::Around {
                game_id: "game:000006".into(),
            },
            5,
        ));
        assert_eq!((window, first), (ids(4..9), 4));
        let (_, window, _, _, _) = ready(index.window(
            &CatalogFrom::Around {
                game_id: "game:000011".into(),
            },
            5,
        ));
        assert_eq!(window, ids(7..12));
        // A game the query does not hold starts at the start.
        let (_, window, _, _, _) = ready(index.window(
            &CatalogFrom::Around {
                game_id: "missing".into(),
            },
            5,
        ));
        assert_eq!(window, ids(0..5));
        // An unreadable cursor is not an error: the window starts at the start.
        let (_, window, _, _, _) = ready(index.window(
            &CatalogFrom::After {
                cursor: Cursor("nonsense".into()),
            },
            3,
        ));
        assert_eq!(window, ids(0..3));
    }

    #[test]
    fn count_is_capped_and_at_least_one() {
        let index = CatalogIndex::new(games(1000), CatalogQuery::default());
        assert_eq!(ready(index.window(&CatalogFrom::Start, 1000)).1.len(), 200);
        assert_eq!(ready(index.window(&CatalogFrom::Start, 0)).1.len(), 1);
    }

    #[test]
    fn filter_is_a_trimmed_case_insensitive_title_substring_within_a_section() {
        let mut list = vec![game(0), game(1), game(2), game(3)];
        list[0].title = "Wario Land 4".into();
        list[1].title = "Super WARIO".into();
        list[2].title = "Zelda".into();
        list[3].title = "wario ware".into();
        list[3].section = Some("Elsewhere".into());
        let games = Arc::new(CatalogGames::new(list));
        let query = CatalogQuery {
            text: "  wArIo ".into(),
            section: None,
            order: CatalogOrder::Korri,
        };
        let index = CatalogIndex::new(games.clone(), query.clone());
        assert_eq!(
            ready(index.window(&CatalogFrom::Start, 50)).1,
            vec!["game:000000", "game:000001", "game:000003"]
        );
        let index = CatalogIndex::new(
            games,
            CatalogQuery {
                section: Some("Elsewhere".into()),
                ..query
            },
        );
        assert_eq!(
            ready(index.window(&CatalogFrom::Start, 50)).1,
            vec!["game:000003"]
        );
    }

    #[test]
    fn orders_match_pico() {
        let mut list: Vec<_> = (0..5).map(game).collect();
        list[0].title = "beta".into();
        list[1].title = "Alpha".into();
        list[2].title = "alpha".into();
        list[3].title = "Gamma".into();
        list[4].title = "delta".into();
        for (index, playtime) in [Some(10.0), None, Some(30.0), Some(0.0), None]
            .into_iter()
            .enumerate()
        {
            list[index].total_playtime_seconds = playtime;
        }
        for (index, last) in [None, Some(5.0), Some(1.0), None, Some(9.0)]
            .into_iter()
            .enumerate()
        {
            list[index].last_played_at = last;
        }
        let games = Arc::new(CatalogGames::new(list));
        let ordered = |order| {
            let index = CatalogIndex::new(
                games.clone(),
                CatalogQuery {
                    text: String::new(),
                    section: None,
                    order,
                },
            );
            ready(index.window(&CatalogFrom::Start, 50))
                .1
                .into_iter()
                .map(|id| id[id.len() - 1..].to_owned())
                .collect::<Vec<_>>()
                .join("")
        };
        assert_eq!(ordered(CatalogOrder::Korri), "01234");
        assert_eq!(ordered(CatalogOrder::Title), "21043");
        // Recorded playtime only; a recorded zero ranks; unknowns last in
        // Korri's order.
        assert_eq!(ordered(CatalogOrder::MostPlayed), "20314");
        assert_eq!(ordered(CatalogOrder::Recent), "41203");
    }

    #[test]
    fn cursors_survive_inserts_elsewhere_and_removal_of_their_game() {
        for order in [
            CatalogOrder::Korri,
            CatalogOrder::Title,
            CatalogOrder::MostPlayed,
            CatalogOrder::Recent,
        ] {
            let query = CatalogQuery {
                text: String::new(),
                section: None,
                order,
            };
            let base: Vec<_> = (0..10_000).map(game).collect();
            let index = CatalogIndex::new(Arc::new(CatalogGames::new(base.clone())), query.clone());
            let (_, first_window, _, _, after) = ready(index.window(
                &CatalogFrom::Around {
                    game_id: "game:005001".into(),
                },
                50,
            ));
            let after = after.unwrap();
            let (_, expected, _, before, _) = ready(index.window(
                &CatalogFrom::After {
                    cursor: after.clone(),
                },
                50,
            ));
            // Insert 500 games at the front, the middle and the end.
            let mut grown = base.clone();
            for (offset, at) in [(20_000, 0), (30_000, 5_000), (40_000, 10_000)] {
                let mut added: Vec<_> = (offset..offset + 500).map(game).collect();
                for game in &mut added {
                    // Sort before everything in every order, so the
                    // inserts are elsewhere in the list.
                    game.title = format!("  {}", game.id);
                    game.total_playtime_seconds = Some(1e12);
                    game.last_played_at = Some(1e12);
                }
                grown.splice(at.min(grown.len())..at.min(grown.len()), added);
            }
            let grown_index =
                CatalogIndex::new(Arc::new(CatalogGames::new(grown.clone())), query.clone());
            let (_, window, _, _, _) = ready(grown_index.window(
                &CatalogFrom::After {
                    cursor: after.clone(),
                },
                50,
            ));
            if order == CatalogOrder::Korri {
                // The middle insert lands inside Korri's order before the
                // cursor's game; the window still starts right after it.
                assert_eq!(window[0], expected[0], "{order:?}");
            } else {
                assert_eq!(window, expected, "{order:?}");
            }
            let (_, window, _, _, _) = ready(grown_index.window(
                &CatalogFrom::Before {
                    cursor: before.clone().unwrap(),
                },
                50,
            ));
            assert_eq!(window.last(), first_window.last(), "{order:?}");
            // The cursor's own game goes away: the window starts where it was.
            let cursor_game = first_window.last().unwrap().clone();
            let without: Vec<_> = base
                .iter()
                .filter(|game| game.id != cursor_game)
                .cloned()
                .collect();
            let without_index = CatalogIndex::new(Arc::new(CatalogGames::new(without)), query);
            let (_, window, _, _, _) =
                ready(without_index.window(&CatalogFrom::After { cursor: after }, 50));
            assert_eq!(window[0], expected[0], "{order:?}");
        }
    }

    /// Timings for the report: building a query (filter and sort) and moving
    /// a window. Run with `--nocapture` to read them.
    #[test]
    fn time_per_move() {
        for size in [12usize, 10_000, 50_000] {
            let games = games(size);
            for order in [
                CatalogOrder::Korri,
                CatalogOrder::Title,
                CatalogOrder::Recent,
            ] {
                let query = CatalogQuery {
                    text: "1".into(),
                    section: None,
                    order,
                };
                let started = Instant::now();
                let index = CatalogIndex::new(games.clone(), query);
                let built = started.elapsed();
                let started = Instant::now();
                let mut cursor = None;
                let moves = 200;
                for _ in 0..moves {
                    let from = match cursor.take() {
                        Some(cursor) => CatalogFrom::After { cursor },
                        None => CatalogFrom::Start,
                    };
                    let SurfaceCatalog::Ready { after, .. } = index.window(&from, 50) else {
                        unreachable!()
                    };
                    cursor = after;
                }
                let moved = started.elapsed() / moves;
                println!(
                    "catalog {size:>6} games {order:?}: query {:>8.3} ms, move {:>8.3} ms",
                    built.as_secs_f64() * 1000.0,
                    moved.as_secs_f64() * 1000.0
                );
                assert!(
                    moved.as_millis() < 250,
                    "{size} {order:?} move took {moved:?}"
                );
            }
        }
    }
}
