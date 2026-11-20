//! Title and id normalization for discovered file-backed releases.

use serde::{Deserialize, Serialize};
use typeshare::typeshare;

use crate::plugin::PluginRegistry;

/// `title.clean` input (ADR 0090): the raw captured `{title}` and the
/// game's system id.
#[typeshare]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginTitleCleanInput {
    pub title: String,
    pub system: String,
}

/// `title.clean` output: the clean title, or `{}` when the plugin gives no
/// answer.
#[typeshare]
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginTitleCleanOutput {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::plugin::deserialize_optional_non_null"
    )]
    pub title: Option<String>,
}

const MAX_TITLE_CHARS: usize = 1024;

/// The clean title of a discovered game (ADR 0090). The enabled plugins that
/// declare `system` are asked in plugin id order, and the first answer wins.
/// A plugin with no `title.clean` handler, a failed call or an unusable
/// answer is no answer. With no answer, Korri's generic cleaning applies.
pub fn clean_discovered_title(registry: &PluginRegistry, raw: &str, system: &str) -> String {
    plugin_title(registry, raw, system).unwrap_or_else(|| clean_title(raw))
}

fn plugin_title(registry: &PluginRegistry, raw: &str, system: &str) -> Option<String> {
    let input = serde_json::to_string(&PluginTitleCleanInput {
        title: raw.to_owned(),
        system: system.to_owned(),
    })
    .ok()?;
    registry
        .system_plugin_ids(system)
        .into_iter()
        .find_map(|plugin_id| {
            let source = registry.plugin_source(plugin_id).ok()?;
            // Presence is kept per source; an absent handler costs no call.
            if !crate::script::has_plugin_operation_snapshot(&source, crate::script::TITLE_CLEAN)
                .ok()?
            {
                return None;
            }
            let output = match crate::script::call_plugin_operation_snapshot(
                &source,
                crate::script::TITLE_CLEAN,
                &input,
            ) {
                Ok(output) => output,
                Err(error) => {
                    eprintln!("korrid discovery: {plugin_id} title.clean failed: {error}");
                    return None;
                }
            };
            let Ok(output) = serde_json::from_str::<PluginTitleCleanOutput>(&output) else {
                eprintln!("korrid discovery: {plugin_id} title.clean answered an unknown shape");
                return None;
            };
            let title = output
                .title?
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            (!title.is_empty()
                && title.chars().count() <= MAX_TITLE_CHARS
                && !title.chars().any(char::is_control))
            .then_some(title)
        })
}

/// The file name without its extension: the raw title when a layout
/// captures none.
pub fn file_stem(file_name: &str, extension: &str) -> String {
    let extension = extension.trim_start_matches('.');
    let mut stem = file_name.to_owned();
    let suffix = format!(".{extension}");
    if stem
        .to_ascii_lowercase()
        .ends_with(&suffix.to_ascii_lowercase())
    {
        let next_len = stem.len().saturating_sub(suffix.len());
        stem.truncate(next_len);
    }
    stem
}

/// Derive the immediate, local title shown before metadata enrichment.
pub fn fallback_title(file_name: &str, extension: &str) -> String {
    clean_title(&file_stem(file_name, extension))
}

/// Korri's generic title cleaning (ADR 0090), used when no system plugin
/// answers: the No-Intro and GoodTools convention. It strips `(…)` and
/// `[…]` groups, turns `_`, `.` and a `-` inside a word into spaces, keeps
/// a ` - ` subtitle separator, moves a trailing `, The`, `, A` or `, An` of
/// the main title to its front, and collapses whitespace.
/// `deploy/title-cleaning-migration/migrate.py` ports these rules; both
/// test against `test-data/title-cleaning.json`.
pub fn clean_title(raw: &str) -> String {
    let stem = raw;
    let mut without_groups = String::new();
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;
    for character in stem.chars() {
        match character {
            '(' if bracket_depth == 0 => paren_depth += 1,
            ')' if bracket_depth == 0 && paren_depth > 0 => paren_depth -= 1,
            '[' if paren_depth == 0 => bracket_depth += 1,
            ']' if paren_depth == 0 && bracket_depth > 0 => bracket_depth -= 1,
            _ if paren_depth == 0 && bracket_depth == 0 => without_groups.push(character),
            _ => {}
        }
    }

    let spaced = without_groups
        .chars()
        .map(|character| match character {
            '_' | '.' => ' ',
            other => other,
        })
        .collect::<Vec<_>>();
    // A `-` with whitespace on both sides separates a subtitle; any other
    // `-` joins two words and becomes a space, as `fallback_title` did.
    let separated = spaced
        .iter()
        .enumerate()
        .map(|(index, character)| {
            let spaced_around = |offset: Option<usize>| {
                offset
                    .and_then(|offset| spaced.get(offset))
                    .is_none_or(|character| character.is_whitespace())
            };
            if *character == '-'
                && !(spaced_around(index.checked_sub(1)) && spaced_around(index.checked_add(1)))
            {
                ' '
            } else {
                *character
            }
        })
        .collect::<String>();
    // Words, with each ` - ` as one `-` word; no leading, trailing or
    // repeated separator.
    let mut parts: Vec<Vec<&str>> = vec![Vec::new()];
    for word in separated.split_whitespace() {
        if word == "-" {
            if parts.last().is_some_and(|part| !part.is_empty()) {
                parts.push(Vec::new());
            }
        } else {
            parts.last_mut().expect("one part").push(word);
        }
    }
    if parts.last().is_some_and(Vec::is_empty) {
        parts.pop();
    }
    let mut parts: Vec<String> = parts.into_iter().map(|words| words.join(" ")).collect();
    if let Some(main) = parts.first_mut() {
        *main = invert_article(main);
    }
    let normalized = parts.join(" - ");

    if normalized.is_empty() {
        stem.split_whitespace().collect::<Vec<_>>().join(" ")
    } else {
        normalized
    }
}

/// `Legend of Zelda, The` becomes `The Legend of Zelda`.
fn invert_article(main: &str) -> String {
    for article in ["The", "A", "An"] {
        if let Some(head) = main.strip_suffix(&format!(", {article}")) {
            if !head.trim().is_empty() {
                return format!("{article} {}", head.trim_end());
            }
        }
    }
    main.to_owned()
}

pub fn normalized_match_name(title: &str) -> String {
    title
        .chars()
        .flat_map(char::to_lowercase)
        .map(|character| {
            if character.is_alphanumeric() {
                character
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn slug_base(title: &str) -> String {
    let mut slug = String::new();
    let mut last_was_sep = false;
    for character in title.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_lowercase() || character.is_ascii_digit() {
            slug.push(character);
            last_was_sep = false;
        } else if !slug.is_empty() && !last_was_sep {
            slug.push('-');
            last_was_sep = true;
        }
    }
    while slug.ends_with('-') || slug.ends_with('.') || slug.ends_with('_') {
        slug.pop();
    }
    if slug.is_empty() || slug == "." || slug == ".." {
        "game".into()
    } else {
        slug
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_dump_decorations_and_keeps_useful_title_text() {
        assert_eq!(
            fallback_title("Wario_Land_4 (USA) [!].GBA", "gba"),
            "Wario Land 4"
        );
        assert_eq!(
            fallback_title("Pokémon - Emerald (Rev 1).gba", "gba"),
            "Pokémon - Emerald"
        );
    }

    #[derive(serde::Deserialize)]
    struct Fixture {
        cases: Vec<(String, String)>,
    }

    /// The cases the one-off migration script also tests against.
    #[test]
    fn cleans_every_shared_fixture_case() {
        let fixture: Fixture =
            serde_json::from_str(include_str!("../../test-data/title-cleaning.json"))
                .expect("fixture");
        assert!(fixture.cases.len() >= 3);
        for (raw, clean) in fixture.cases {
            assert_eq!(clean_title(&raw), clean, "{raw:?}");
        }
    }

    /// An installed plugin `@test:<name>` from its `plugin.ts` source.
    fn package(
        root: &std::path::Path,
        name: &str,
        source: &str,
    ) -> crate::plugin_installation::EnabledPackage {
        let folder = root.join(name);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("plugin.ts"), source).unwrap();
        crate::plugin_installation::EnabledPackage {
            id: format!("@test:{name}"),
            package: folder,
            files: Default::default(),
            entry: "plugin.ts".into(),
            sources: vec!["plugin.ts".into()],
            requires: Vec::new(),
            hosts: Vec::new(),
            credential_host: None,
        }
    }

    /// A plugin that declares `system` and cleans every title to `answer`
    /// (`undefined`: it answers `{}`).
    fn cleaner(name: &str, system: &str, answer: &str) -> String {
        let source = format!(
            r#"export const name = "{name}";
export const systems = {{ {system}: {{ title: "S", content: {{ extensions: ["bin"] }} }} }};
export const handlers = {{
  "title.clean": (input) => {{
    if (Object.keys(input).sort().join() !== "system,title") throw new Error("input");
    const title = {answer};
    return title === undefined ? {{}} : {{ title: title + " [" + input.system + "]" }};
  }},
}};
"#
        );
        source
    }

    fn registry(root: &std::path::Path, plugins: &[(&str, &str)]) -> PluginRegistry {
        PluginRegistry::from_installed(
            plugins
                .iter()
                .map(|(name, source)| package(root, name, source))
                .collect(),
        )
        .unwrap()
    }

    #[test]
    fn a_plugin_answer_beats_the_generic_cleaning() {
        let root = tempfile::tempdir().unwrap();
        let registry = registry(
            root.path(),
            &[("mame", &cleaner("mame", "arcade", "\"Pac-Man\""))],
        );
        assert_eq!(
            clean_discovered_title(&registry, "pacman (USA)", "arcade"),
            "Pac-Man [arcade]"
        );
    }

    #[test]
    fn plugin_id_order_decides_and_an_empty_answer_passes_on() {
        let root = tempfile::tempdir().unwrap();
        let registry = registry(
            root.path(),
            &[
                ("c", &cleaner("c", "snes", "\"Third\"")),
                ("a", &cleaner("a", "snes", "undefined")),
                ("b", &cleaner("b", "snes", "\"Second\"")),
            ],
        );
        assert_eq!(
            registry.system_plugin_ids("snes"),
            ["@test:a", "@test:b", "@test:c"]
        );
        assert_eq!(
            clean_discovered_title(&registry, "Secret of Mana (U) [!]", "snes"),
            "Second [snes]"
        );
    }

    #[test]
    fn a_plugin_that_does_not_declare_the_system_is_not_asked() {
        let root = tempfile::tempdir().unwrap();
        let registry = registry(
            root.path(),
            &[
                ("other", &cleaner("other", "gba", "\"Wrong\"")),
                ("plain", "export const name = \"plain\"; export const systems = { snes: { content: { extensions: [\"sfc\"] } } };"),
            ],
        );
        assert_eq!(
            clean_discovered_title(&registry, "Secret of Mana (U) [!]", "snes"),
            "Secret of Mana"
        );
    }

    #[test]
    fn a_failed_or_blank_answer_is_no_answer() {
        let root = tempfile::tempdir().unwrap();
        let registry = registry(
            root.path(),
            &[
                ("a", "export const name = \"a\"; export const systems = { gb: {} }; export const handlers = { \"title.clean\": () => { throw new Error(\"no\") } };"),
                ("b", "export const name = \"b\"; export const systems = { gb: {} }; export const handlers = { \"title.clean\": () => ({ title: \"   \" }) };"),
                ("c", "export const name = \"c\"; export const systems = { gb: {} }; export const handlers = { \"title.clean\": () => ({ name: \"x\" }) };"),
            ],
        );
        assert_eq!(
            clean_discovered_title(&registry, "Tetris (World) (Rev 1)", "gb"),
            "Tetris"
        );
    }

    #[test]
    fn cleans_ibuki_shaped_titles() {
        assert_eq!(clean_title("Secret of Mana (U) [!]"), "Secret of Mana");
        assert_eq!(
            clean_title("Legend of Zelda, The - Oracle of Ages (USA)"),
            "The Legend of Zelda - Oracle of Ages"
        );
        assert_eq!(
            clean_title("Metroid Fusion (USA, Australia)"),
            "Metroid Fusion"
        );
    }

    #[test]
    fn creates_schema_safe_slugs_with_fallback() {
        assert_eq!(slug_base("Pokémon Emerald"), "pok-mon-emerald");
        assert_eq!(slug_base("🎮"), "game");
        assert_eq!(slug_base("007: GoldenEye"), "007-goldeneye");
    }

    #[test]
    fn normalizes_provider_match_names_like_fallback_titles() {
        assert_eq!(normalized_match_name("Wario-Land 4™"), "wario land 4");
        assert_eq!(
            normalized_match_name(" Pokémon: Emerald "),
            "pokémon emerald"
        );
    }
}
