//! A launch's named steps (contract amendment A4). The list is planned when the
//! launch begins, from what korrid knows then: the save check is a step only
//! when a peer is in reach, the download only when the game needs one. The
//! launch then moves through it; a step that turns out to be unneeded is
//! passed at once, and a step never comes back. Unknown peer runner steps may
//! replace Start exactly once when that session reports them.

use korri_surface_treaty::SurfaceSteps;

/// The stages of a launch the person waits on, in launch order.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum LaunchStep {
    /// The save check asks the peers in reach before the launch.
    CheckSaves,
    /// Missing content is downloaded.
    Download,
    /// korrid resolves the game, its settings and its runner's command, and
    /// starts the game unit.
    Prepare,
    /// The game unit runs until the game takes over the screen.
    Start,
}

impl LaunchStep {
    fn label(self) -> &'static str {
        match self {
            Self::CheckSaves => "Checking saves",
            Self::Download => "Downloading",
            Self::Prepare => "Getting the game ready",
            Self::Start => "Starting the game",
        }
    }
}

/// One launch's steps and the one running now.
#[derive(Clone, Debug, PartialEq)]
pub struct LaunchPlan {
    game_id: String,
    steps: Vec<(LaunchStep, String)>,
    current: usize,
    runner_steps_known: bool,
}

impl LaunchPlan {
    pub fn new(game_id: &str, peer_in_reach: bool, needs_download: bool) -> Self {
        Self::with_runner_steps(game_id, peer_in_reach, needs_download, None)
    }

    pub fn with_runner_steps(
        game_id: &str,
        peer_in_reach: bool,
        needs_download: bool,
        runner_steps: Option<&[crate::plugin::RunnerStep]>,
    ) -> Self {
        let mut steps: Vec<_> = [
            peer_in_reach.then_some(LaunchStep::CheckSaves),
            needs_download.then_some(LaunchStep::Download),
            Some(LaunchStep::Prepare),
            Some(LaunchStep::Start),
        ]
        .into_iter()
        .flatten()
        .map(|step| (step, step.label().to_owned()))
        .collect();
        if let Some(runner) = runner_steps.filter(|steps| !steps.is_empty()) {
            steps.pop();
            steps.extend(
                runner
                    .iter()
                    .map(|step| (LaunchStep::Start, step.label.clone())),
            );
        }
        Self {
            game_id: game_id.to_owned(),
            steps,
            current: 0,
            runner_steps_known: runner_steps.is_some(),
        }
    }

    /// korrid's id of the launched game.
    pub fn game_id(&self) -> &str {
        &self.game_id
    }

    /// The launch reached `step`. A step the plan does not list moves to the
    /// next listed one. The current step never moves back.
    pub fn reach(&mut self, step: LaunchStep) {
        if let Some(index) = self.steps.iter().position(|(listed, _)| *listed >= step) {
            self.current = self.current.max(index);
        }
    }

    pub fn current(&self) -> LaunchStep {
        self.steps[self.current].0
    }

    /// Only Start can be amended, only once. Later observations move within
    /// that exact list, never grow it or move its current index backwards.
    pub fn follow_runner(&mut self, reported: &SurfaceSteps) {
        if self.current() != LaunchStep::Start
            || reported.labels.is_empty()
            || reported.current as usize >= reported.labels.len()
        {
            return;
        }
        let start = self
            .steps
            .iter()
            .position(|(step, _)| *step == LaunchStep::Start)
            .expect("Start listed");
        if !self.runner_steps_known {
            self.steps.truncate(start);
            self.steps.extend(
                reported
                    .labels
                    .iter()
                    .cloned()
                    .map(|label| (LaunchStep::Start, label)),
            );
            self.runner_steps_known = true;
        }
        if self.steps[start..]
            .iter()
            .map(|(_, label)| label)
            .eq(reported.labels.iter())
        {
            self.current = self.current.max(start + reported.current as usize);
        }
    }

    pub fn surface_steps(&self) -> SurfaceSteps {
        SurfaceSteps {
            labels: self.steps.iter().map(|(_, label)| label.clone()).collect(),
            current: self.current as u32,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(plan: &LaunchPlan) -> Vec<String> {
        plan.surface_steps().labels
    }

    #[test]
    fn plan_lists_only_the_steps_that_run() {
        assert_eq!(
            labels(&LaunchPlan::new("g", false, false)),
            ["Getting the game ready", "Starting the game"]
        );
        assert_eq!(
            labels(&LaunchPlan::new("g", true, true)),
            [
                "Checking saves",
                "Downloading",
                "Getting the game ready",
                "Starting the game"
            ]
        );
    }

    #[test]
    fn an_unlisted_step_moves_to_the_next_listed_step_and_never_back() {
        let mut plan = LaunchPlan::new("g", true, false);
        assert_eq!(plan.current(), LaunchStep::CheckSaves);
        plan.reach(LaunchStep::Download);
        assert_eq!(plan.current(), LaunchStep::Prepare);
        assert_eq!(plan.surface_steps().current, 1);
        plan.reach(LaunchStep::Start);
        plan.reach(LaunchStep::CheckSaves);
        assert_eq!(plan.current(), LaunchStep::Start);
        assert_eq!(plan.surface_steps().labels.len(), 3);
    }
    #[test]
    fn runner_steps_replace_start_at_plan_time() {
        let runner = vec![
            crate::plugin::RunnerStep {
                key: "steam".into(),
                label: "Starting Steam".into(),
            },
            crate::plugin::RunnerStep {
                key: "game".into(),
                label: "Starting the game".into(),
            },
        ];
        let mut plan = LaunchPlan::with_runner_steps("g", true, true, Some(&runner));
        assert_eq!(
            labels(&plan),
            [
                "Checking saves",
                "Downloading",
                "Getting the game ready",
                "Starting Steam",
                "Starting the game"
            ]
        );
        plan.reach(LaunchStep::Start);
        plan.follow_runner(&SurfaceSteps {
            labels: runner.iter().map(|step| step.label.clone()).collect(),
            current: 1,
        });
        assert_eq!(plan.surface_steps().current, 4);
        plan.reach(LaunchStep::Start);
        assert_eq!(plan.surface_steps().current, 4);
    }

    #[test]
    fn unknown_runner_can_amend_start_once_and_only_at_start() {
        let mut plan = LaunchPlan::new("g", false, false);
        let report = SurfaceSteps {
            labels: vec![
                "Starting Steam".into(),
                "Signing in".into(),
                "Starting the game".into(),
            ],
            current: 1,
        };
        plan.follow_runner(&report);
        assert_eq!(labels(&plan).len(), 2);
        plan.reach(LaunchStep::Start);
        plan.follow_runner(&report);
        assert_eq!(
            labels(&plan),
            [
                "Getting the game ready",
                "Starting Steam",
                "Signing in",
                "Starting the game"
            ]
        );
        assert_eq!(plan.surface_steps().current, 2);
        plan.follow_runner(&SurfaceSteps {
            labels: report.labels.clone(),
            current: 0,
        });
        assert_eq!(plan.surface_steps().current, 2);
        plan.follow_runner(&SurfaceSteps {
            labels: vec!["Changed".into()],
            current: 0,
        });
        assert_eq!(labels(&plan).len(), 4);
        plan.follow_runner(&SurfaceSteps {
            current: 2,
            ..report
        });
        assert_eq!(plan.surface_steps().current, 3);
    }

    #[test]
    fn known_runner_never_amends_even_when_it_has_no_steps() {
        let mut plan = LaunchPlan::with_runner_steps("g", false, false, Some(&[]));
        plan.reach(LaunchStep::Start);
        plan.follow_runner(&SurfaceSteps {
            labels: vec!["Other".into()],
            current: 0,
        });
        assert_eq!(
            labels(&plan),
            ["Getting the game ready", "Starting the game"]
        );
    }
}
