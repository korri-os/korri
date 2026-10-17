use regex::Regex;

use crate::plugin::ContentRules;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct Captures {
    pub title: Option<String>,
    pub disc: Option<String>,
    pub version: Option<String>,
    pub region: Option<String>,
}

pub(super) fn extension<'a>(name: &str, rules: &'a ContentRules) -> Option<&'a str> {
    let lower = name.to_ascii_lowercase();
    rules
        .extensions
        .iter()
        .filter(|ext| lower.ends_with(&format!(".{}", ext.to_ascii_lowercase())))
        .max_by_key(|ext| ext.len())
        .map(String::as_str)
}

pub(super) fn match_layout(path: &str, ext: &str, rules: &ContentRules) -> Option<Captures> {
    let defaults = vec!["**/{title}.{ext}".to_owned()];
    let layouts = if rules.layouts.is_empty() {
        &defaults
    } else {
        &rules.layouts
    };
    layouts.iter().find_map(|pattern| {
        let expression = expression(pattern, ext)?;
        let regex = Regex::new(&expression).ok()?;
        let captures = regex.captures(path)?;
        let capture = |name| captures.name(name).map(|value| value.as_str().to_owned());
        Some(Captures {
            title: capture("title"),
            disc: capture("disc"),
            version: capture("version"),
            region: capture("region"),
        })
    })
}

fn expression(pattern: &str, ext: &str) -> Option<String> {
    let mut expression = String::from("^");
    let segments: Vec<_> = pattern.split('/').collect();
    let mut previous_globstar = false;
    let mut tokens = std::collections::BTreeSet::new();
    for (index, segment) in segments.iter().enumerate() {
        let terminal_globstar = *segment == "**" && index + 1 == segments.len();
        if terminal_globstar {
            expression.push_str(if index == 0 || previous_globstar {
                "(?:[^/]+(?:/[^/]+)*)?"
            } else {
                "(?:/[^/]+)*"
            });
            continue;
        }
        if index > 0 && !previous_globstar {
            expression.push('/');
        }
        if *segment == "**" {
            // A globstar consumes whole segments including their separators.
            expression.push_str("(?:[^/]+/)*");
            previous_globstar = true;
            continue;
        }
        previous_globstar = false;
        let mut chars = segment.chars().peekable();
        while let Some(character) = chars.next() {
            match character {
                '*' => expression.push_str("[^/]*"),
                '{' => {
                    let token: String = chars.by_ref().take_while(|ch| *ch != '}').collect();
                    match token.as_str() {
                        "title" | "disc" | "version" | "region" => {
                            if tokens.insert(token.clone()) {
                                expression.push_str(&format!("(?P<{token}>[^/]+?)"));
                            } else {
                                // The first occurrence owns a capture, as the
                                // first matching layout owns all captures.
                                expression.push_str("[^/]+?");
                            }
                        }
                        "ext" => expression.push_str(&format!("(?i:{})", regex::escape(ext))),
                        _ => return None,
                    }
                }
                other => expression.push_str(&regex::escape(&other.to_string())),
            }
        }
    }
    expression.push('$');
    Some(expression)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_ordered_patterns_and_zero_or_many_whole_segments() {
        let mut rules = ContentRules {
            extensions: vec!["png".into(), "p8.png".into()],
            layouts: vec![
                "**/{title} (Disc {disc}) [{region}] v{version}.{ext}".into(),
                "**/{title}.{ext}".into(),
            ],
            ..ContentRules::default()
        };
        for path in [
            "Game (Disc 2) [USA] v1.1.P8.PNG",
            "a/b/Game (Disc 2) [USA] v1.1.P8.PNG",
        ] {
            let ext = extension(path.rsplit('/').next().unwrap(), &rules).unwrap();
            assert_eq!(ext, "p8.png");
            let matched = match_layout(path, ext, &rules).unwrap();
            assert_eq!(matched.title.as_deref(), Some("Game"));
            assert_eq!(matched.disc.as_deref(), Some("2"));
            assert_eq!(matched.region.as_deref(), Some("USA"));
            assert_eq!(matched.version.as_deref(), Some("1.1"));
        }
        rules.layouts = vec!["sets/*/{title}.{ext}".into()];
        assert!(match_layout("sets/a/Game.png", "png", &rules).is_some());
        assert!(match_layout("sets/a/b/Game.png", "png", &rules).is_none());
        rules.layouts = vec![
            "**/{title}.{ext}".into(),
            "{title} (Disc {disc}).{ext}".into(),
        ];
        let matched = match_layout("Game (Disc 1).png", "png", &rules).unwrap();
        assert_eq!(matched.title.as_deref(), Some("Game (Disc 1)"));
        assert_eq!(matched.disc, None);
    }
}
