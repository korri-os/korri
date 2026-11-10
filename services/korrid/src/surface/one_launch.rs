//! One caller's transient settings. No file writes and no shared global draft.
use crate::{
    config::cascade::{set_leaf, LaunchSettingValue},
    launch_settings::{LaunchOverride, LaunchRunnerScope, LaunchScopedSettings},
};
use std::collections::HashMap;

#[derive(Clone)]
struct Draft {
    version: u64,
    settings: LaunchScopedSettings,
}

#[derive(Default)]
pub(crate) struct OneLaunch {
    sequence: u64,
    pending: HashMap<String, Draft>,
    submitted: HashMap<u64, (String, Draft)>,
    last: Option<(u64, String, Draft)>,
}
impl OneLaunch {
    fn next(&mut self) -> u64 {
        self.sequence = self
            .sequence
            .checked_add(1)
            .expect("one-launch sequence exhausted");
        self.sequence
    }
    pub fn pending(&self, game: &str) -> Option<&LaunchScopedSettings> {
        self.pending.get(game).map(|draft| &draft.settings)
    }
    pub fn keepable(&self, game: &str) -> Option<&LaunchScopedSettings> {
        self.pending(game).or_else(|| {
            self.last
                .as_ref()
                .filter(|(_, id, _)| id == game)
                .map(|(_, _, draft)| &draft.settings)
        })
    }
    pub fn set(
        &mut self,
        game: &str,
        scope: LaunchRunnerScope,
        path: &str,
        value: Option<LaunchSettingValue>,
    ) -> Result<(), String> {
        if matches!(scope, LaunchRunnerScope::Any) && path.starts_with("plugin.") {
            return Err("Runner-only settings need a runner scope.".into());
        }
        if self.pending(game).is_some_and(|draft| draft.scope != scope) {
            return Err(
                "The one-launch draft uses another scope. Clear it before changing scope.".into(),
            );
        }
        let version = self.next();
        let draft = self.pending.entry(game.into()).or_insert_with(|| Draft {
            version,
            settings: LaunchScopedSettings {
                scope,
                values: HashMap::new(),
            },
        });
        set_leaf(&mut draft.settings.values, path, value);
        draft.version = version;
        if draft.settings.values.is_empty() {
            self.pending.remove(game);
        }
        Ok(())
    }
    /// The ticket identifies exactly the values sent, not a later editor state.
    pub fn submit(&mut self, game: &str) -> (u64, Option<LaunchOverride>) {
        let Some(draft) = self.pending.get(game).cloned() else {
            return (0, None);
        };
        let ticket = self.next();
        let launch = LaunchOverride {
            settings: Some(draft.settings.clone()),
            ..Default::default()
        };
        self.submitted.insert(ticket, (game.into(), draft));
        (ticket, Some(launch))
    }
    pub fn complete(&mut self, ticket: u64, success: bool) {
        let Some((game, draft)) = self.submitted.remove(&ticket) else {
            return;
        };
        if !success {
            return;
        }
        if self
            .pending
            .get(&game)
            .is_some_and(|current| current.version == draft.version)
        {
            self.pending.remove(&game);
        }
        if self
            .last
            .as_ref()
            .is_none_or(|(known, _, _)| ticket > *known)
        {
            self.last = Some((ticket, game, draft));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::cascade::leaf;
    fn scale(value: u64) -> LaunchSettingValue {
        LaunchSettingValue::Number(value.into())
    }
    #[test]
    fn successful_launch_consumes_once_but_retains_values_for_keep() {
        let mut drafts = OneLaunch::default();
        drafts
            .set(
                "game",
                LaunchRunnerScope::Any,
                "video.render-scale",
                Some(scale(4)),
            )
            .unwrap();
        let (ticket, sent) = drafts.submit("game");
        assert_eq!(
            leaf(
                &sent.unwrap().settings.unwrap().values,
                "video.render-scale"
            ),
            Some(&scale(4))
        );
        drafts.complete(ticket, true);
        assert!(drafts.submit("game").1.is_none());
        assert_eq!(
            leaf(
                &drafts.keepable("game").unwrap().values,
                "video.render-scale"
            ),
            Some(&scale(4))
        );
        assert!(drafts.keepable("other-game").is_none());
    }
    #[test]
    fn failure_keeps_the_draft_and_late_success_cannot_erase_new_edits() {
        let mut drafts = OneLaunch::default();
        drafts
            .set(
                "game",
                LaunchRunnerScope::Any,
                "video.render-scale",
                Some(scale(4)),
            )
            .unwrap();
        let (failed, _) = drafts.submit("game");
        drafts.complete(failed, false);
        let (late, _) = drafts.submit("game");
        drafts
            .set(
                "game",
                LaunchRunnerScope::Any,
                "video.render-scale",
                Some(scale(2)),
            )
            .unwrap();
        drafts.complete(late, true);
        assert_eq!(
            leaf(
                &drafts.pending("game").unwrap().values,
                "video.render-scale"
            ),
            Some(&scale(2))
        );
    }
    #[test]
    fn removed_values_do_not_return_after_an_inflight_launch() {
        let mut drafts = OneLaunch::default();
        drafts
            .set(
                "game",
                LaunchRunnerScope::Any,
                "video.render-scale",
                Some(scale(4)),
            )
            .unwrap();
        let (ticket, _) = drafts.submit("game");
        drafts
            .set("game", LaunchRunnerScope::Any, "video.render-scale", None)
            .unwrap();
        drafts.complete(ticket, true);
        assert!(drafts.pending("game").is_none());
    }
}
