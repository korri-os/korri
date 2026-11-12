//! A cursor catalog over a fixed list of games (contract 3.4), for the
//! install-time verifier and the fixture korrid. korrid owns the real one;
//! this exists so a package can be exercised against 0, 12 and 10,000 games
//! without korrid.
//!
//! Cursors are opaque to surfaces. Here they are `i<position>` in the
//! filtered, ordered list.

use serde_json::{json, Value};

pub const DEFAULT_COUNT: usize = 50;
pub const MAX_COUNT: usize = 200;

pub fn default_query() -> Value {
    json!({"text": "", "section": null, "order": "korri"})
}

fn title(game: &Value) -> String {
    game.get("title")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_lowercase()
}

fn number(game: &Value, key: &str) -> f64 {
    game.get(key)
        .and_then(Value::as_f64)
        .unwrap_or(f64::NEG_INFINITY)
}

/// The games a query selects, in its order.
pub fn select<'a>(games: &'a [Value], query: &Value) -> Vec<&'a Value> {
    let text = query
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_lowercase();
    let section = query.get("section").and_then(Value::as_str);
    // As korrid: only games whose `systemId` equals the query's. A game
    // without one is in no system.
    let system_id = query.get("systemId").and_then(Value::as_str);
    let mut selected: Vec<&Value> = games
        .iter()
        .filter(|game| text.is_empty() || title(game).contains(&text))
        .filter(|game| section.is_none() || game.get("section").and_then(Value::as_str) == section)
        .filter(|game| {
            system_id.is_none() || game.get("systemId").and_then(Value::as_str) == system_id
        })
        .collect();
    match query
        .get("order")
        .and_then(Value::as_str)
        .unwrap_or("korri")
    {
        "title" => selected.sort_by_key(|game| title(game)),
        "mostPlayed" => {
            selected.sort_by(|a, b| number(b, "playCount").total_cmp(&number(a, "playCount")))
        }
        "recent" => {
            selected.sort_by(|a, b| number(b, "lastPlayedAt").total_cmp(&number(a, "lastPlayedAt")))
        }
        _ => {}
    }
    selected
}

/// `SurfaceModel.systems` as korrid publishes it: the systems of the whole
/// list with their game counts, whatever a surface's query is, in title
/// order. A game without a `systemId` is in none of them. The first game of
/// a system gives its words (`system`, else the id).
pub fn systems(games: &[Value]) -> Vec<Value> {
    // (system id, words, count), in first-seen order until sorted.
    let mut systems: Vec<(String, String, u64)> = Vec::new();
    for game in games {
        let Some(id) = game.get("systemId").and_then(Value::as_str) else {
            continue;
        };
        match systems.iter_mut().find(|(known, _, _)| known == id) {
            Some((_, _, total)) => *total += 1,
            None => {
                let words = game.get("system").and_then(Value::as_str).unwrap_or(id);
                systems.push((id.to_owned(), words.to_owned(), 1));
            }
        }
    }
    // korrid's title order: lowercase first, then "a" before "A", then the id.
    systems.sort_by(|left, right| {
        left.1
            .to_lowercase()
            .cmp(&right.1.to_lowercase())
            .then_with(|| right.1.cmp(&left.1))
            .then_with(|| left.0.cmp(&right.0))
    });
    systems
        .into_iter()
        .map(|(id, words, total)| json!({"systemId": id, "system": words, "total": total}))
        .collect()
}

fn cursor_position(cursor: Option<&Value>) -> Option<usize> {
    cursor?.as_str()?.strip_prefix('i')?.parse().ok()
}

/// Answer a `MoveCatalog { query, from, count }` with a `SurfaceCatalog`.
pub fn window(games: &[Value], query: &Value, from: &Value, count: usize) -> Value {
    if games.is_empty() {
        return json!({"_tag": "Empty"});
    }
    let count = count.clamp(1, MAX_COUNT);
    let selected = select(games, query);
    let total = selected.len();
    let start = match from.get("_tag").and_then(Value::as_str) {
        Some("End") => total.saturating_sub(count),
        Some("After") => cursor_position(from.get("cursor")).map_or(0, |p| (p + 1).min(total)),
        Some("Before") => {
            cursor_position(from.get("cursor")).map_or(0, |p| p.min(total).saturating_sub(count))
        }
        Some("Around") => {
            let id = from.get("gameId").and_then(Value::as_str);
            selected
                .iter()
                .position(|game| game.get("id").and_then(Value::as_str) == id)
                .map_or(0, |p| {
                    p.saturating_sub(count / 2).min(total.saturating_sub(count))
                })
        }
        _ => 0,
    };
    let end = (start + count).min(total);
    let window: Vec<Value> = selected[start..end].iter().map(|g| (*g).clone()).collect();
    let mut ready = json!({
        "_tag": "Ready",
        "total": total,
        "window": window,
        "firstIndex": start,
        "query": query,
    });
    if start > 0 {
        ready["before"] = json!(format!("i{start}"));
    }
    if end < total {
        ready["after"] = json!(format!("i{}", end - 1));
    }
    ready
}

/// `count` fixture games cycled from the given examples, with unique ids
/// and titles.
pub fn games(examples: &[Value], count: usize) -> Vec<Value> {
    (0..count)
        .map(|i| {
            let mut game = examples[i % examples.len()].clone();
            let base_id = game["id"].as_str().unwrap_or("game").to_owned();
            let base_title = game["title"].as_str().unwrap_or("Game").to_owned();
            if i >= examples.len() {
                game["id"] = json!(format!("{base_id}-{i}"));
                game["title"] = json!(format!("{base_title} {i}"));
            }
            game
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn twelve() -> Vec<Value> {
        (0..12).map(|i| json!({"id": format!("g{i}"), "title": format!("Game {i:02}"), "playCount": i, "section": if i % 2 == 0 { "even" } else { "odd" }})).collect()
    }

    #[test]
    fn windows_move_with_cursors() {
        let games = twelve();
        let first = window(&games, &default_query(), &json!({"_tag": "Start"}), 5);
        assert_eq!(first["total"], 12);
        assert_eq!(first["window"].as_array().unwrap().len(), 5);
        assert!(first.get("before").is_none());
        let next = window(
            &games,
            &default_query(),
            &json!({"_tag": "After", "cursor": first["after"]}),
            5,
        );
        assert_eq!(next["firstIndex"], 5);
        assert_eq!(next["window"][0]["id"], "g5");
        let end = window(&games, &default_query(), &json!({"_tag": "End"}), 5);
        assert_eq!(end["firstIndex"], 7);
        assert!(end.get("after").is_none());
        let back = window(
            &games,
            &default_query(),
            &json!({"_tag": "Before", "cursor": end["before"]}),
            5,
        );
        assert_eq!(back["firstIndex"], 2);
        let around = window(
            &games,
            &default_query(),
            &json!({"_tag": "Around", "gameId": "g6"}),
            5,
        );
        assert_eq!(around["firstIndex"], 4);
    }

    #[test]
    fn queries_filter_and_order() {
        let games = twelve();
        let query = json!({"text": " game 1", "section": null, "order": "korri"});
        assert_eq!(
            window(&games, &query, &json!({"_tag": "Start"}), 50)["total"],
            2
        ); // Game 10, Game 11
        let query = json!({"text": "", "section": "odd", "order": "mostPlayed"});
        let result = window(&games, &query, &json!({"_tag": "Start"}), 50);
        assert_eq!(result["total"], 6);
        assert_eq!(result["window"][0]["id"], "g11");
        assert_eq!(
            window(&[], &default_query(), &json!({"_tag": "Start"}), 50),
            json!({"_tag": "Empty"})
        );
        assert_eq!(super::games(&twelve(), 10_000).len(), 10_000);
    }

    /// Twelve games: every third on the GBA, every fourth (that is not a
    /// third) on the NES, the rest with no system, as a peer's game has none.
    fn with_systems() -> Vec<Value> {
        twelve()
            .into_iter()
            .enumerate()
            .map(|(i, mut game)| {
                if i % 3 == 0 {
                    game["systemId"] = json!("gba");
                    game["system"] = json!("Game Boy Advance");
                } else if i % 4 == 0 {
                    game["systemId"] = json!("nes");
                    game["system"] = json!("NES");
                }
                game
            })
            .collect()
    }

    fn ids(result: &Value) -> Vec<&str> {
        result["window"]
            .as_array()
            .unwrap()
            .iter()
            .map(|game| game["id"].as_str().unwrap())
            .collect()
    }

    #[test]
    fn a_system_filter_keeps_only_that_systems_games() {
        let games = with_systems();
        let start = json!({"_tag": "Start"});
        let query = json!({"text": "", "systemId": "gba", "order": "korri"});
        let result = window(&games, &query, &start, 50);
        assert_eq!(result["total"], 4);
        assert_eq!(ids(&result), ["g0", "g3", "g6", "g9"]);
        assert!(result["window"]
            .as_array()
            .unwrap()
            .iter()
            .all(|game| game["systemId"] == "gba"));
        // The window says which query it answers.
        assert_eq!(result["query"]["systemId"], "gba");
        // It combines with the section, the text and the order.
        let query =
            json!({"text": "game 0", "section": "odd", "systemId": "gba", "order": "mostPlayed"});
        assert_eq!(ids(&window(&games, &query, &start, 50)), ["g9", "g3"]);
        let query = json!({"text": "", "section": null, "systemId": "nes", "order": "korri"});
        assert_eq!(ids(&window(&games, &query, &start, 50)), ["g4", "g8"]);
        // An id no game has matches nothing. It is not "every system".
        let query = json!({"text": "", "systemId": "snes", "order": "korri"});
        let none = window(&games, &query, &start, 50);
        assert_eq!(
            (&none["_tag"], &none["total"]),
            (&json!("Ready"), &json!(0))
        );
        // Absent, or null as a package may send it: every system, also the
        // games with none.
        for query in [
            default_query(),
            json!({"text": "", "systemId": null, "order": "korri"}),
        ] {
            assert_eq!(window(&games, &query, &start, 50)["total"], 12);
        }
        // Cursors move inside the filtered list.
        let query = json!({"text": "", "systemId": "gba", "order": "korri"});
        let first = window(&games, &query, &start, 2);
        assert_eq!(ids(&first), ["g0", "g3"]);
        let next = window(
            &games,
            &query,
            &json!({"_tag": "After", "cursor": first["after"]}),
            2,
        );
        assert_eq!(ids(&next), ["g6", "g9"]);
    }

    #[test]
    fn systems_are_counted_over_the_whole_list_in_title_order() {
        assert_eq!(
            systems(&with_systems()),
            vec![
                json!({"systemId": "gba", "system": "Game Boy Advance", "total": 4}),
                json!({"systemId": "nes", "system": "NES", "total": 2}),
            ]
        );
        // No game with a system, and no game at all: an empty list.
        assert!(systems(&twelve()).is_empty());
        assert!(systems(&[]).is_empty());
        // Title order as korrid sorts it, not id order and not first-seen
        // order. A system with no words is named by its id.
        let games = vec![
            json!({"id": "a", "title": "A", "systemId": "zx"}),
            json!({"id": "b", "title": "B", "systemId": "pico8", "system": "PICO-8"}),
            json!({"id": "c", "title": "C", "systemId": "gb", "system": "game boy"}),
            json!({"id": "d", "title": "D", "systemId": "gbc", "system": "Game Boy Color"}),
            json!({"id": "e", "title": "E", "systemId": "pico8", "system": "PICO-8"}),
            json!({"id": "f", "title": "F"}),
        ];
        assert_eq!(
            systems(&games),
            vec![
                json!({"systemId": "gb", "system": "game boy", "total": 1}),
                json!({"systemId": "gbc", "system": "Game Boy Color", "total": 1}),
                json!({"systemId": "pico8", "system": "PICO-8", "total": 2}),
                json!({"systemId": "zx", "system": "zx", "total": 1}),
            ]
        );
        // The list is of the whole list: no query narrows it.
        assert_eq!(systems(&games).len(), 4);
    }
}
