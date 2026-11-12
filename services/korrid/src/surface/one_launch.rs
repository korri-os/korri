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

#[derive(Clone, Default)]
struct Draft {
    version: u64,
    changes: Vec<Change>,
    /// The keep place the person chose. None: the list's default, this game.
    keep: Option<String>,
}

#[derive(Default)]
pub(crate) struct OneLaunch {
    sequence: u64,
    pending: HashMap<String, Draft>,
    submitted: HashMap<u64, (String, Draft)>,
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
        if self
            .pending
            .get(game)
            .is_some_and(|draft| draft.changes.is_empty())
        {
            // The keep choice belongs to changes; with none it starts again.
            self.pending.remove(game);
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
        if self.pending.contains_key(game) {
            self.touch(game).keep = Some(place.into());
        }
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
    /// The values the next launch carries, when the person keeps the changes
    /// for this launch only. The ticket identifies exactly the values sent,
    /// not a later editor state.
    pub fn submit(&mut self, game: &str) -> (u64, Option<LaunchOverride>) {
        let Some(draft) = self
            .pending
            .get(game)
            .filter(|draft| draft.keep.as_deref() == Some(KEEP_LAUNCH))
            .cloned()
        else {
            return (0, None);
        };
        let Ok(launch) = launch_override(&draft.changes) else {
            return (0, None);
        };
        if launch.is_empty() {
            return (0, None);
        }
        let ticket = self.next();
        self.submitted.insert(ticket, (game.into(), draft));
        (ticket, Some(launch))
    }
    /// A launch took its values: they end with it. A stream viewer's change
    /// was not the launch's to carry, so it still waits.
    pub fn complete(&mut self, ticket: u64, success: bool) {
        let Some((game, draft)) = self.submitted.remove(&ticket) else {
            return;
        };
        if !success {
            return;
        }
        if let Some(current) = self
            .pending
            .get_mut(&game)
            .filter(|current| current.version == draft.version)
        {
            current
                .changes
                .retain(|change| matches!(change.side, Side::Viewer { .. }));
        }
        self.settle(&game);
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
    fn for_launch(drafts: &mut OneLaunch) {
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
            drafts.submit("game").1.is_none(),
            "this game is the default"
        );
        for_launch(&mut drafts);
        let (ticket, launch) = drafts.submit("game");
        assert_eq!(sent(launch), Some(LaunchSettingValue::Number(4.into())));
        drafts.complete(ticket, true);
        assert!(
            drafts.changes("game").is_empty(),
            "they end with the launch"
        );
        assert!(drafts.submit("game").1.is_none());
        assert!(drafts.submit("other-game").1.is_none());
    }

    #[test]
    fn failure_keeps_the_changes_and_late_success_cannot_erase_new_ones() {
        let mut drafts = OneLaunch::default();
        drafts.set("game", scale(4));
        for_launch(&mut drafts);
        let (failed, _) = drafts.submit("game");
        drafts.complete(failed, false);
        let (late, _) = drafts.submit("game");
        drafts.set("game", scale(2));
        drafts.complete(late, true);
        assert_eq!(drafts.changes("game"), [scale(2)]);
    }

    #[test]
    fn a_row_set_back_leaves_no_change_and_no_keep_choice() {
        let mut drafts = OneLaunch::default();
        drafts.set("game", scale(4));
        for_launch(&mut drafts);
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

    #[test]
    fn a_launch_does_not_carry_a_stream_viewers_change_and_does_not_end_it() {
        let mut drafts = OneLaunch::default();
        drafts.set("game", codec());
        for_launch(&mut drafts);
        assert!(
            drafts.submit("game").1.is_none(),
            "a stream value alone is nothing to carry"
        );
        drafts.set("game", scale(4));
        let (ticket, launch) = drafts.submit("game");
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
                row: "launch:row:setting:plugin.speed".into(),
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
