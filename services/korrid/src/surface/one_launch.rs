//! One game's changes that are not kept yet (ADR 0066), and where the person
//! wants them kept. No file writes and no shared global draft: the editor
//! writes them when it closes, and a launch carries the ones for this launch.
use crate::{
    config::cascade::{set_leaf, LaunchSettingValue},
    launch_settings::{LaunchOverride, LaunchPath},
};
use serde_json::Value;
use std::collections::HashMap;

use super::launch_model::list::KEEP_LAUNCH;

/// A game by the device that runs it. A streamed copy and this device's own
/// copy can share an id, and their changes must not mix.
pub(crate) fn game_key(id: &str, host: Option<&str>) -> String {
    match host {
        Some(host) => format!("{host}\u{0}{id}"),
        None => id.to_owned(),
    }
}

/// The device a value belongs to, and so where its change is kept.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Side {
    /// The device that runs the game. Its change is kept in the place the
    /// person chose, or goes with one launch.
    Holder {
        /// The runner a runner-only setting belongs to.
        runner: Option<String>,
    },
    /// This device's own stream viewer. Its change is always kept on this
    /// device, in the place the viewer reads: this device has no game place
    /// and no one-launch place for a streamed game's viewer (issue #395).
    Viewer {
        /// The viewer: a runner of this device.
        runner: String,
    },
}

/// One changed row.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Change {
    /// The list row that made it. A row has at most one change.
    pub row: String,
    /// korrid's typed path (`LaunchPath`), as JSON.
    pub path: Value,
    /// The value to use. Null: the plugin's own default.
    pub value: Value,
    pub side: Side,
}

impl Change {
    /// The runner of a runner-only setting of the device that runs the game.
    fn only_for(&self) -> Option<&str> {
        match &self.side {
            Side::Holder { runner } => runner.as_deref(),
            Side::Viewer { .. } => None,
        }
    }
}

/// A runner-only change set aside while another runner is chosen.
#[derive(Clone)]
struct Aside {
    change: Change,
    /// The keep place in force when the change was set aside. The keep
    /// control it was made under is not in view while it waits.
    keep: Option<String>,
}

#[derive(Clone, Default)]
struct Draft {
    version: u64,
    changes: Vec<Change>,
    /// Runner-only changes for a runner that is not the chosen one. They
    /// wait out of sight: no list shows them and no close writes them,
    /// until their runner is chosen again. Only a launch that uses their
    /// runner carries them.
    aside: Vec<Aside>,
    /// The keep place the person chose. None: the list's default, this game.
    keep: Option<String>,
}

fn for_launch(keep: &Option<String>) -> bool {
    keep.as_deref() == Some(KEEP_LAUNCH)
}

/// What one launch carried, by its ticket.
struct Carried {
    game: String,
    /// The draft's version when the launch took the changes.
    version: u64,
    changes: Vec<Change>,
}

#[derive(Default)]
pub(crate) struct OneLaunch {
    sequence: u64,
    pending: HashMap<String, Draft>,
    submitted: HashMap<u64, Carried>,
}

/// The changes as the values of one launch. A shared setting has no runner
/// scope; a runner-only setting is keyed by its runner. A stream viewer's
/// value is no launch value: it is left out.
pub(crate) fn launch_override(changes: &[Change]) -> Result<LaunchOverride, String> {
    let mut launch = LaunchOverride::default();
    let setting = |value: &Value| -> Result<Option<LaunchSettingValue>, String> {
        if value.is_null() {
            return Ok(Some(LaunchSettingValue::Cleared));
        }
        serde_json::from_value(value.clone())
            .map(Some)
            .map_err(|error| error.to_string())
    };
    let percent = |value: &Value| -> Result<u8, String> {
        value
            .as_u64()
            .and_then(|percent| u8::try_from(percent).ok())
            .ok_or_else(|| "A limit is a whole percent.".to_owned())
    };
    for change in changes {
        let Side::Holder { runner } = &change.side else {
            continue;
        };
        let path: LaunchPath =
            serde_json::from_value(change.path.clone()).map_err(|error| error.to_string())?;
        match path {
            LaunchPath::Setting { key } => match key.strip_prefix("plugin.") {
                Some(rest) => {
                    let runner = runner
                        .clone()
                        .ok_or("A runner-only setting needs its runner.")?;
                    set_leaf(
                        launch.plugin.entry(runner).or_default(),
                        rest,
                        setting(&change.value)?,
                    );
                }
                None => set_leaf(&mut launch.settings, &key, setting(&change.value)?),
            },
            LaunchPath::ModifierEnabled { id } => {
                launch.modifiers.entry(id).or_default().enabled = change.value.as_bool();
            }
            LaunchPath::ModifierSetting { id, key } => set_leaf(
                &mut launch.modifiers.entry(id).or_default().settings,
                &key,
                setting(&change.value)?,
            ),
            LaunchPath::ServiceSetting { id, key } => set_leaf(
                &mut launch.services.entry(id).or_default().settings,
                &key,
                setting(&change.value)?,
            ),
            LaunchPath::CpuMin => {
                launch.cpu.get_or_insert_with(Default::default).min = Some(percent(&change.value)?);
            }
            LaunchPath::CpuMax => {
                launch.cpu.get_or_insert_with(Default::default).max = Some(percent(&change.value)?);
            }
            LaunchPath::CpuGovernor => {
                launch.cpu.get_or_insert_with(Default::default).governor =
                    change.value.as_str().map(str::to_owned);
            }
            LaunchPath::GpuMin => {
                launch.gpu.get_or_insert_with(Default::default).min = Some(percent(&change.value)?);
            }
            LaunchPath::GpuMax => {
                launch.gpu.get_or_insert_with(Default::default).max = Some(percent(&change.value)?);
            }
            // The runner is chosen in the runner chooser, never as a change.
            LaunchPath::Runner => return Err("A runner is not a launch value.".into()),
        }
    }
    Ok(launch)
}

impl OneLaunch {
    fn next(&mut self) -> u64 {
        self.sequence = self
            .sequence
            .checked_add(1)
            .expect("one-launch sequence exhausted");
        self.sequence
    }
    fn touch(&mut self, game: &str) -> &mut Draft {
        let version = self.next();
        let draft = self.pending.entry(game.into()).or_default();
        draft.version = version;
        draft
    }
    fn settle(&mut self, game: &str) {
        let Some(draft) = self.pending.get_mut(game) else {
            return;
        };
        if draft.changes.is_empty() {
            // The keep choice belongs to changes; with none it starts again.
            draft.keep = None;
            if draft.aside.is_empty() {
                self.pending.remove(game);
            }
        }
    }
    pub fn changes(&self, game: &str) -> &[Change] {
        self.pending
            .get(game)
            .map_or(&[], |draft| draft.changes.as_slice())
    }
    pub fn keep(&self, game: &str) -> Option<&str> {
        self.pending
            .get(game)
            .and_then(|draft| draft.keep.as_deref())
    }
    pub fn set_keep(&mut self, game: &str, place: &str) {
        if !self.changes(game).is_empty() {
            self.touch(game).keep = Some(place.into());
        }
    }
    /// The runner the game has chosen now, and the runners it offers. A
    /// runner-only change belongs to one runner: while another runner is
    /// chosen it is set aside, and it is a change again once its own runner
    /// is chosen. A change for a runner the game no longer offers is dropped.
    pub fn choose_runner(&mut self, game: &str, chosen: Option<&str>, offered: &[&str]) {
        let Some(draft) = self.pending.get_mut(game) else {
            return;
        };
        let (stay, leave): (Vec<Change>, Vec<Change>) = std::mem::take(&mut draft.changes)
            .into_iter()
            .partition(|change| {
                change
                    .only_for()
                    .is_none_or(|runner| Some(runner) == chosen)
            });
        let (back, wait): (Vec<Aside>, Vec<Aside>) = std::mem::take(&mut draft.aside)
            .into_iter()
            .partition(|aside| aside.change.only_for() == chosen);
        let keep = draft.keep.clone();
        if let Some(first) = back.first().filter(|_| stay.is_empty()) {
            // No other change shows: the keep place is the one these left under.
            draft.keep = first.keep.clone();
        }
        draft.changes = stay
            .into_iter()
            .chain(back.into_iter().map(|aside| aside.change))
            .collect();
        draft.aside = wait
            .into_iter()
            .chain(leave.into_iter().map(|change| Aside {
                change,
                keep: keep.clone(),
            }))
            .filter(|aside| {
                aside
                    .change
                    .only_for()
                    .is_some_and(|runner| offered.contains(&runner))
            })
            .collect();
        self.settle(game);
    }
    /// One row's change. The row's earlier change, if any, is replaced.
    pub fn set(&mut self, game: &str, change: Change) {
        let draft = self.touch(game);
        match draft
            .changes
            .iter_mut()
            .find(|known| known.row == change.row)
        {
            Some(known) => *known = change,
            None => draft.changes.push(change),
        }
    }
    /// The row is back at its earlier value.
    pub fn remove(&mut self, game: &str, row: &str) {
        if self.pending.contains_key(game) {
            self.touch(game).changes.retain(|change| change.row != row);
            self.settle(game);
        }
    }
    /// A change that was written, unless the person changed the row again.
    pub fn kept(&mut self, game: &str, written: &Change) {
        if let Some(draft) = self.pending.get_mut(game) {
            draft.changes.retain(|change| change != written);
        }
        self.settle(game);
    }
    /// Drop the changes the list cannot show: a row that is gone, or a value
    /// the device now has anyway. They could not be seen or set back.
    pub fn retain(&mut self, game: &str, shown: impl Fn(&Change) -> bool) {
        if let Some(draft) = self.pending.get_mut(game) {
            draft.changes.retain(|change| shown(change));
        }
        self.settle(game);
    }
    /// The values the next launch carries: the changes the person keeps for
    /// this launch only. They are the shared changes, and the runner-only
    /// changes of the runner the launch uses, shown or set aside. `runner`
    /// is that runner. None: the launch names no runner, so it uses the
    /// runner the game had chosen when its list was last read. The ticket
    /// identifies exactly the changes sent, not a later editor state.
    pub fn submit(&mut self, game: &str, runner: Option<&str>) -> (u64, Option<LaunchOverride>) {
        let Some(draft) = self.pending.get(game) else {
            return (0, None);
        };
        let shown = draft
            .changes
            .iter()
            .filter(|_| for_launch(&draft.keep))
            .filter(|change| match (change.only_for(), runner) {
                (Some(only_for), Some(runner)) => only_for == runner,
                _ => true,
            });
        let aside = draft
            .aside
            .iter()
            .filter(|aside| for_launch(&aside.keep))
            .map(|aside| &aside.change)
            .filter(|change| runner.is_some() && change.only_for() == runner);
        let changes: Vec<Change> = shown
            .chain(aside)
            // A stream viewer's value is not a launch's to carry.
            .filter(|change| matches!(change.side, Side::Holder { .. }))
            .cloned()
            .collect();
        let version = draft.version;
        let Ok(launch) = launch_override(&changes) else {
            return (0, None);
        };
        if launch.is_empty() {
            return (0, None);
        }
        let ticket = self.next();
        self.submitted.insert(
            ticket,
            Carried {
                game: game.into(),
                version,
                changes,
            },
        );
        (ticket, Some(launch))
    }
    /// A launch took its values: what it carried ends with it. Every other
    /// change still waits: another runner's, and a stream viewer's.
    pub fn complete(&mut self, ticket: u64, success: bool) {
        let Some(carried) = self.submitted.remove(&ticket) else {
            return;
        };
        if !success {
            return;
        }
        // The person changed the draft after the launch took it: it is a new
        // draft, and the launch ends none of it.
        if let Some(draft) = self
            .pending
            .get_mut(&carried.game)
            .filter(|draft| draft.version == carried.version)
        {
            draft
                .changes
                .retain(|change| !carried.changes.contains(change));
            draft
                .aside
                .retain(|aside| !carried.changes.contains(&aside.change));
        }
        self.settle(&carried.game);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::cascade::leaf;
    use serde_json::json;

    fn scale(value: u64) -> Change {
        Change {
            row: "launch:row:setting:video.render-scale".into(),
            path: json!({"_tag":"Setting","payload":{"key":"video.render-scale"}}),
            value: json!(value),
            side: Side::Holder { runner: None },
        }
    }
    /// This device's stream viewer codec.
    fn codec() -> Change {
        Change {
            row: "launch:stream:@korri:moonlight/moonlight#video.codec".into(),
            path: json!({"_tag":"Setting","payload":{"key":"video.codec"}}),
            value: json!("av1"),
            side: Side::Viewer {
                runner: "@korri:moonlight/moonlight".into(),
            },
        }
    }
    fn keep_for_launch(drafts: &mut OneLaunch) {
        drafts.set_keep("game", KEEP_LAUNCH);
    }
    fn sent(launch: Option<LaunchOverride>) -> Option<LaunchSettingValue> {
        leaf(&launch?.settings, "video.render-scale").cloned()
    }

    #[test]
    fn a_launch_carries_the_changes_only_when_they_are_kept_for_this_launch() {
        let mut drafts = OneLaunch::default();
        drafts.set("game", scale(4));
        assert!(
            drafts.submit("game", None).1.is_none(),
            "this game is the default"
        );
        keep_for_launch(&mut drafts);
        let (ticket, launch) = drafts.submit("game", None);
        assert_eq!(sent(launch), Some(LaunchSettingValue::Number(4.into())));
        drafts.complete(ticket, true);
        assert!(
            drafts.changes("game").is_empty(),
            "they end with the launch"
        );
        assert!(drafts.submit("game", None).1.is_none());
        assert!(drafts.submit("other-game", None).1.is_none());
    }

    #[test]
    fn failure_keeps_the_changes_and_late_success_cannot_erase_new_ones() {
        let mut drafts = OneLaunch::default();
        drafts.set("game", scale(4));
        keep_for_launch(&mut drafts);
        let (failed, _) = drafts.submit("game", None);
        drafts.complete(failed, false);
        let (late, _) = drafts.submit("game", None);
        drafts.set("game", scale(2));
        drafts.complete(late, true);
        assert_eq!(drafts.changes("game"), [scale(2)]);
    }

    #[test]
    fn a_row_set_back_leaves_no_change_and_no_keep_choice() {
        let mut drafts = OneLaunch::default();
        drafts.set("game", scale(4));
        keep_for_launch(&mut drafts);
        drafts.remove("game", &scale(4).row);
        assert!(drafts.changes("game").is_empty());
        drafts.set("game", scale(2));
        assert_eq!(drafts.keep("game"), None);
    }

    #[test]
    fn a_written_change_goes_unless_the_row_changed_again() {
        let mut drafts = OneLaunch::default();
        drafts.set("game", scale(4));
        let written = scale(4);
        drafts.set("game", scale(2));
        drafts.kept("game", &written);
        assert_eq!(drafts.changes("game"), [scale(2)]);
        drafts.kept("game", &scale(2));
        assert!(drafts.changes("game").is_empty());
    }

    fn speed(runner: &str, value: u64) -> Change {
        Change {
            row: format!("launch:row:runner:{runner}:setting:plugin.speed"),
            path: json!({"_tag":"Setting","payload":{"key":"plugin.speed"}}),
            value: json!(value),
            side: Side::Holder {
                runner: Some(runner.into()),
            },
        }
    }

    #[test]
    fn a_runner_only_change_is_set_aside_until_its_runner_is_chosen_again() {
        let mut drafts = OneLaunch::default();
        let offered = ["mgba", "gpsp"];
        drafts.set("game", speed("mgba", 3));
        drafts.set("game", scale(4));
        keep_for_launch(&mut drafts);
        drafts.choose_runner("game", Some("mgba"), &offered);
        assert_eq!(drafts.changes("game"), [speed("mgba", 3), scale(4)]);

        // Another runner: the change is no change of the list, or of a
        // launch with that runner.
        drafts.choose_runner("game", Some("gpsp"), &offered);
        assert_eq!(drafts.changes("game"), [scale(4)]);
        let (ticket, launch) = drafts.submit("game", None);
        assert!(launch.unwrap().plugin.is_empty());
        drafts.complete(ticket, true);
        assert!(drafts.changes("game").is_empty());
        // Nothing shows now, so there is no keep choice to change or to hold.
        drafts.set_keep("game", "game:wario");
        assert_eq!(drafts.keep("game"), None);

        // No runner chosen: it still waits. Its own runner: it is back.
        drafts.choose_runner("game", None, &offered);
        assert!(drafts.changes("game").is_empty());
        drafts.set("game", speed("gpsp", 7));
        drafts.choose_runner("game", Some("mgba"), &offered);
        assert_eq!(drafts.changes("game"), [speed("mgba", 3)]);
        // It is back with the keep place it left under.
        assert_eq!(drafts.keep("game"), Some(KEEP_LAUNCH));
        drafts.choose_runner("game", Some("gpsp"), &offered);
        assert_eq!(drafts.changes("game"), [speed("gpsp", 7)]);
        assert_eq!(drafts.keep("game"), None);

        // A runner the game no longer offers: its change is dropped.
        drafts.choose_runner("game", Some("gpsp"), &["gpsp"]);
        drafts.kept("game", &speed("gpsp", 7));
        drafts.choose_runner("game", Some("mgba"), &offered);
        assert!(drafts.changes("game").is_empty());
    }

    fn plugin_of(launch: Option<LaunchOverride>) -> Value {
        serde_json::to_value(launch.expect("one-launch values").plugin).unwrap()
    }

    #[test]
    fn a_launch_carries_the_runner_only_changes_of_the_runner_it_uses_and_ends_only_those() {
        let offered = ["mgba", "gpsp"];
        let mut drafts = OneLaunch::default();
        drafts.set("game", speed("mgba", 3));
        keep_for_launch(&mut drafts);
        // gpSP is saved: mGBA's change is set aside, gpSP has its own.
        drafts.choose_runner("game", Some("gpsp"), &offered);
        drafts.set("game", speed("gpsp", 7));
        drafts.set("game", scale(4));
        drafts.set("game", codec());
        keep_for_launch(&mut drafts);

        // Launch once with mGBA: the shared change and mGBA's.
        let (ticket, launch) = drafts.submit("game", Some("mgba"));
        assert_eq!(
            serde_json::to_value(launch.unwrap()).unwrap(),
            json!({
                "settings": { "video": { "render-scale": 4 } },
                "plugin": { "mgba": { "speed": 3 } },
                "modifiers": {},
            })
        );
        // A launch that failed ends nothing.
        drafts.complete(ticket, false);
        let (ticket, _) = drafts.submit("game", Some("mgba"));
        drafts.complete(ticket, true);
        // gpSP's change was not used: it still waits. So does the viewer's.
        assert_eq!(drafts.changes("game"), [speed("gpsp", 7), codec()]);
        drafts.choose_runner("game", Some("mgba"), &offered);
        assert_eq!(drafts.changes("game"), [codec()], "mGBA's change ended");

        // A launch that names no runner uses the saved one.
        drafts.choose_runner("game", Some("gpsp"), &offered);
        let (ticket, launch) = drafts.submit("game", None);
        assert_eq!(plugin_of(launch), json!({ "gpsp": { "speed": 7 } }));
        drafts.complete(ticket, true);
        assert_eq!(drafts.changes("game"), [codec()]);
    }

    #[test]
    fn a_launch_with_the_saved_runner_leaves_another_runners_change_set_aside() {
        let offered = ["mgba", "gpsp"];
        let mut drafts = OneLaunch::default();
        drafts.set("game", speed("mgba", 3));
        keep_for_launch(&mut drafts);
        drafts.choose_runner("game", Some("gpsp"), &offered);
        drafts.set("game", speed("gpsp", 7));
        keep_for_launch(&mut drafts);
        let (ticket, launch) = drafts.submit("game", Some("gpsp"));
        assert_eq!(plugin_of(launch), json!({ "gpsp": { "speed": 7 } }));
        drafts.complete(ticket, true);
        assert!(drafts.changes("game").is_empty());
        // A runner that has no change of its own carries nothing.
        assert!(drafts.submit("game", Some("gpsp")).1.is_none());
        drafts.choose_runner("game", Some("mgba"), &offered);
        assert_eq!(drafts.changes("game"), [speed("mgba", 3)]);
        assert_eq!(drafts.keep("game"), Some(KEEP_LAUNCH));
    }

    #[test]
    fn a_set_aside_change_goes_with_a_launch_only_when_it_was_for_this_launch() {
        let offered = ["mgba", "gpsp"];
        let mut drafts = OneLaunch::default();
        // mGBA's change is for this game, the default.
        drafts.set("game", speed("mgba", 3));
        drafts.choose_runner("game", Some("gpsp"), &offered);
        drafts.set("game", scale(4));
        keep_for_launch(&mut drafts);
        // Launch once with mGBA: the shared change only.
        let (ticket, launch) = drafts.submit("game", Some("mgba"));
        assert_eq!(plugin_of(launch), json!({}));
        drafts.complete(ticket, true);
        assert!(drafts.changes("game").is_empty());
        // It waits for mGBA, and is still for this game.
        drafts.choose_runner("game", Some("mgba"), &offered);
        assert_eq!(drafts.changes("game"), [speed("mgba", 3)]);
        assert_eq!(drafts.keep("game"), None);
    }

    #[test]
    fn a_launch_ends_what_it_carried_also_when_a_set_aside_change_came_back_meanwhile() {
        let mut drafts = OneLaunch::default();
        let offered = ["mgba", "gpsp"];
        drafts.set("game", speed("mgba", 3));
        drafts.choose_runner("game", Some("gpsp"), &offered);
        drafts.set("game", scale(4));
        keep_for_launch(&mut drafts);
        let (ticket, _) = drafts.submit("game", Some("gpsp"));
        drafts.choose_runner("game", Some("mgba"), &offered);
        drafts.complete(ticket, true);
        assert_eq!(drafts.changes("game"), [speed("mgba", 3)]);
    }

    #[test]
    fn a_launch_does_not_carry_a_stream_viewers_change_and_does_not_end_it() {
        let mut drafts = OneLaunch::default();
        drafts.set("game", codec());
        keep_for_launch(&mut drafts);
        assert!(
            drafts.submit("game", None).1.is_none(),
            "a stream value alone is nothing to carry"
        );
        drafts.set("game", scale(4));
        let (ticket, launch) = drafts.submit("game", None);
        assert_eq!(
            serde_json::to_value(launch.unwrap()).unwrap(),
            json!({ "settings": { "video": { "render-scale": 4 } }, "modifiers": {} })
        );
        drafts.complete(ticket, true);
        assert_eq!(drafts.changes("game"), [codec()]);
    }

    #[test]
    fn one_launch_values_have_no_runner_scope_and_runner_only_values_name_their_runner() {
        let launch = launch_override(&[
            scale(3),
            Change {
                row: "launch:row:runner:@test:runner/main:setting:plugin.speed".into(),
                path: json!({"_tag":"Setting","payload":{"key":"plugin.speed"}}),
                value: json!(2),
                side: Side::Holder {
                    runner: Some("@test:runner/main".into()),
                },
            },
            Change {
                row: "launch:row:setting:video.vsync".into(),
                path: json!({"_tag":"Setting","payload":{"key":"video.vsync"}}),
                value: Value::Null,
                side: Side::Holder { runner: None },
            },
            Change {
                row: "launch:row:cpu.governor".into(),
                path: json!({"_tag":"CpuGovernor"}),
                value: json!("performance"),
                side: Side::Holder { runner: None },
            },
            Change {
                row: "launch:row:cpu.max".into(),
                path: json!({"_tag":"CpuMax"}),
                value: json!(60),
                side: Side::Holder { runner: None },
            },
            Change {
                row: "launch:row:service:@korri:sunshine/sunshine:setting:video.bitrate".into(),
                path: json!({"_tag":"ServiceSetting","payload":{"id":"@korri:sunshine/sunshine","key":"video.bitrate"}}),
                value: json!(20),
                side: Side::Holder { runner: None },
            },
            Change {
                row: "launch:row:modifier:@korri:gamescope/gamescope:enabled".into(),
                path: json!({"_tag":"ModifierEnabled","payload":{"id":"@korri:gamescope/gamescope"}}),
                value: json!(true),
                side: Side::Holder { runner: None },
            },
        ])
        .unwrap();
        assert_eq!(
            serde_json::to_value(&launch).unwrap(),
            json!({
                "settings": {"video": {"render-scale": 3, "vsync": null}},
                "plugin": {"@test:runner/main": {"speed": 2}},
                "modifiers": {"@korri:gamescope/gamescope": {"enabled": true, "settings": {}}},
                "services": {"@korri:sunshine/sunshine": {"settings": {"video": {"bitrate": 20}}}},
                "cpu": {"max": 60, "governor": "performance"},
            })
        );
        assert!(launch_override(&[Change {
            row: "launch:row:setting:plugin.speed".into(),
            path: json!({"_tag":"Setting","payload":{"key":"plugin.speed"}}),
            value: json!(2),
            side: Side::Holder { runner: None },
        }])
        .is_err());
    }
}
