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
    game.get("title").and_then(Value::as_str).unwrap_or("").to_lowercase()
}

fn number(game: &Value, key: &str) -> f64 {
    game.get(key).and_then(Value::as_f64).unwrap_or(f64::NEG_INFINITY)
}

/// The games a query selects, in its order.
pub fn select<'a>(games: &'a [Value], query: &Value) -> Vec<&'a Value> {
    let text = query.get("text").and_then(Value::as_str).unwrap_or("").trim().to_lowercase();
    let section = query.get("section").and_then(Value::as_str);
    let mut selected: Vec<&Value> = games
        .iter()
        .filter(|game| text.is_empty() || title(game).contains(&text))
        .filter(|game| section.is_none() || game.get("section").and_then(Value::as_str) == section)
        .collect();
    match query.get("order").and_then(Value::as_str).unwrap_or("korri") {
        "title" => selected.sort_by_key(|game| title(game)),
        "mostPlayed" => selected.sort_by(|a, b| number(b, "playCount").total_cmp(&number(a, "playCount"))),
        "recent" => selected.sort_by(|a, b| number(b, "lastPlayedAt").total_cmp(&number(a, "lastPlayedAt"))),
        _ => {}
    }
    selected
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
        Some("Before") => cursor_position(from.get("cursor")).map_or(0, |p| p.min(total).saturating_sub(count)),
        Some("Around") => {
            let id = from.get("gameId").and_then(Value::as_str);
            selected
                .iter()
                .position(|game| game.get("id").and_then(Value::as_str) == id)
                .map_or(0, |p| p.saturating_sub(count / 2).min(total.saturating_sub(count)))
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
        let next = window(&games, &default_query(), &json!({"_tag": "After", "cursor": first["after"]}), 5);
        assert_eq!(next["firstIndex"], 5);
        assert_eq!(next["window"][0]["id"], "g5");
        let end = window(&games, &default_query(), &json!({"_tag": "End"}), 5);
        assert_eq!(end["firstIndex"], 7);
        assert!(end.get("after").is_none());
        let back = window(&games, &default_query(), &json!({"_tag": "Before", "cursor": end["before"]}), 5);
        assert_eq!(back["firstIndex"], 2);
        let around = window(&games, &default_query(), &json!({"_tag": "Around", "gameId": "g6"}), 5);
        assert_eq!(around["firstIndex"], 4);
    }

    #[test]
    fn queries_filter_and_order() {
        let games = twelve();
        let query = json!({"text": " game 1", "section": null, "order": "korri"});
        assert_eq!(window(&games, &query, &json!({"_tag": "Start"}), 50)["total"], 2); // Game 10, Game 11
        let query = json!({"text": "", "section": "odd", "order": "mostPlayed"});
        let result = window(&games, &query, &json!({"_tag": "Start"}), 50);
        assert_eq!(result["total"], 6);
        assert_eq!(result["window"][0]["id"], "g11");
        assert_eq!(window(&[], &default_query(), &json!({"_tag": "Start"}), 50), json!({"_tag": "Empty"}));
        assert_eq!(super::games(&twelve(), 10_000).len(), 10_000);
    }
}
