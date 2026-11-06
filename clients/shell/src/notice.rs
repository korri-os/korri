//! The notice the shell shows after it unloads a surface (contract amendment
//! A5). The shell never switches surfaces on its own: it names the surface and
//! the reason in plain words and offers two actions, try the surface again or
//! use another one (the previous surface when there is one). It switches only
//! on the person's choice. With no other surface, it offers "try again" only.
//!
//! The failed surface cannot draw, so the shell draws the notice from its own
//! `notice.slint`. Up and down move the focus, confirm takes the focused
//! action, back takes "use the other surface".

use crate::{
    engine::{HEAP_LIMIT, STUCK_LIMIT},
    messages::Button,
};
use anyhow::{anyhow, Result};
use slint_interpreter::{ComponentDefinition, ComponentInstance, SharedString, Value};

/// Why the shell unloaded a surface, or could not load it from the notice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    /// A call was still running at the stuck limit.
    Stuck,
    /// The QuickJS heap reached its limit.
    Memory,
    /// The logic threw, broke the contract, or drew a view the shell refused.
    Error,
    /// A load chosen from the notice failed.
    NotStarted,
}

/// What the person chose.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Choice {
    /// Load this surface again.
    Retry(String),
    /// Load this other surface.
    Use(String),
}

/// The notice's content and focus: a pure function of the failed surface, the
/// reason and the other surface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    pub surface: String,
    pub reason: Reason,
    pub other: Option<String>,
    pub focused: usize,
}

/// The longest name the notice shows, in characters. A longer id (ids may have 64) is cut
/// with an ellipsis, so the notice fits a 320x240 window; the log keeps the whole id.
pub const NAME_CHARS: usize = 20;

/// A surface id as a person reads it: `pico` is "Pico".
pub fn display_name(id: &str) -> String {
    let mut chars = id.chars();
    let name: String = match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    };
    if name.chars().count() <= NAME_CHARS {
        return name;
    }
    let mut cut: String = name.chars().take(NAME_CHARS - 1).collect();
    cut.push('…');
    cut
}

impl Notice {
    pub fn new(surface: &str, reason: Reason, other: Option<&str>) -> Self {
        Self {
            surface: surface.to_owned(),
            reason,
            other: other.map(str::to_owned),
            focused: 0,
        }
    }

    pub fn heading(&self) -> String {
        let name = display_name(&self.surface);
        match self.reason {
            Reason::Stuck => format!("{name} stopped responding"),
            Reason::Memory => format!("{name} ran out of memory"),
            Reason::Error => format!("{name} stopped because of an error"),
            Reason::NotStarted => format!("{name} could not start"),
        }
    }

    pub fn detail(&self) -> String {
        let name = display_name(&self.surface);
        match self.reason {
            Reason::Stuck => format!(
                "{name} was still working after {} seconds, so Korri closed it.",
                STUCK_LIMIT.as_secs()
            ),
            Reason::Memory => format!(
                "{name} used more than {} MiB, so Korri closed it.",
                HEAP_LIMIT / (1024 * 1024)
            ),
            Reason::Error => format!("{name} hit an error in its own code, so Korri closed it."),
            Reason::NotStarted => format!("Korri tried to open {name}, and it failed."),
        }
    }

    /// The actions in order, with their labels: try again first, then the other surface.
    pub fn actions(&self) -> Vec<(String, Choice)> {
        let mut actions = vec![(
            format!("Try {} again", display_name(&self.surface)),
            Choice::Retry(self.surface.clone()),
        )];
        if let Some(other) = &self.other {
            actions.push((
                format!("Use {}", display_name(other)),
                Choice::Use(other.clone()),
            ));
        }
        actions
    }

    /// A press on the notice: up and down move the focus (no wrap), confirm takes the
    /// focused action, back takes "use the other surface". Other buttons do nothing.
    pub fn press(&mut self, button: Button) -> Option<Choice> {
        let actions = self.actions();
        match button {
            Button::Up => {
                self.focused = self.focused.saturating_sub(1);
                None
            }
            Button::Down => {
                self.focused = (self.focused + 1).min(actions.len() - 1);
                None
            }
            Button::Confirm => actions.into_iter().nth(self.focused).map(|(_, c)| c),
            Button::Back => self.other.clone().map(Choice::Use),
            _ => None,
        }
    }
}

/// Inter, unmodified (`notice-fonts/README.md`): the notice must draw on a device with
/// no system fonts.
static FONT: &[u8] = include_bytes!("notice-fonts/Inter-VariableFont.ttf");

thread_local! {
    static FONT_REGISTERED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Register the notice's font in Slint's shared collection, once per thread.
fn register_font() {
    if FONT_REGISTERED.with(|r| r.replace(true)) {
        return;
    }
    let mut collection = slint::fontique_011::shared_collection();
    collection.register_fonts(fontique::Blob::new(std::sync::Arc::new(FONT)), None);
}

/// Compile `notice.slint`. The shell does it once, the first time it needs a notice.
pub fn compile() -> Result<ComponentDefinition> {
    register_font();
    let compiler = slint_interpreter::Compiler::default();
    let result = pollster::block_on(
        compiler.build_from_source(include_str!("notice.slint").into(), "notice.slint".into()),
    );
    if result.has_errors() {
        let diagnostics: Vec<String> = result.diagnostics().map(|d| d.to_string()).collect();
        return Err(anyhow!(
            "notice.slint does not compile: {}",
            diagnostics.join("; ")
        ));
    }
    result
        .component("Notice")
        .ok_or_else(|| anyhow!("notice.slint has no Notice"))
}

/// Set the component's properties from `notice`.
pub fn bind(component: &ComponentInstance, notice: &Notice) -> Result<()> {
    let text = |s: String| Value::String(SharedString::from(s));
    let actions: Vec<Value> = notice
        .actions()
        .into_iter()
        .map(|(label, _)| text(label))
        .collect();
    component.set_property("heading", text(notice.heading()))?;
    component.set_property("detail", text(notice.detail()))?;
    component.set_property(
        "actions",
        Value::Model(slint::ModelRc::new(slint::VecModel::from(actions))),
    )?;
    component.set_property("focused", Value::Number(notice.focused as f64))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_surface_and_the_reason_in_plain_words() {
        let notice = Notice::new("pico", Reason::Stuck, Some("zone"));
        assert_eq!(notice.heading(), "Pico stopped responding");
        assert_eq!(
            notice.detail(),
            "Pico was still working after 2 seconds, so Korri closed it."
        );
        let labels: Vec<String> = notice.actions().into_iter().map(|(l, _)| l).collect();
        assert_eq!(labels, ["Try Pico again", "Use Zone"]);
        assert_eq!(
            Notice::new("pico", Reason::Memory, None).detail(),
            "Pico used more than 64 MiB, so Korri closed it."
        );
        assert_eq!(
            display_name(&"a".repeat(20)),
            format!("A{}", "a".repeat(19))
        );
        assert_eq!(
            display_name(&"a".repeat(64)),
            format!("A{}…", "a".repeat(18))
        );
    }

    #[test]
    fn up_down_confirm_and_back_choose() {
        let mut notice = Notice::new("pico", Reason::Error, Some("zone"));
        assert_eq!(notice.press(Button::Up), None);
        assert_eq!(notice.focused, 0);
        assert_eq!(notice.press(Button::Down), None);
        assert_eq!(notice.press(Button::Down), None);
        assert_eq!(notice.focused, 1, "no wrap");
        for button in [Button::Left, Button::Right, Button::Options, Button::Menu] {
            assert_eq!(notice.press(button), None);
        }
        assert_eq!(
            notice.press(Button::Confirm),
            Some(Choice::Use("zone".into()))
        );
        assert_eq!(notice.press(Button::Up), None);
        assert_eq!(
            notice.press(Button::Confirm),
            Some(Choice::Retry("pico".into()))
        );
        assert_eq!(notice.press(Button::Back), Some(Choice::Use("zone".into())));
    }

    #[test]
    fn with_no_other_surface_only_try_again() {
        let mut notice = Notice::new("pico", Reason::Stuck, None);
        assert_eq!(notice.actions().len(), 1);
        assert_eq!(notice.press(Button::Down), None);
        assert_eq!(notice.focused, 0);
        assert_eq!(notice.press(Button::Back), None);
        assert_eq!(
            notice.press(Button::Confirm),
            Some(Choice::Retry("pico".into()))
        );
    }
}
