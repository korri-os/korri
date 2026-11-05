//! A launch's named steps (contract amendment A4). The list is fixed when the
//! launch begins, from what korrid knows then: the save check is a step only
//! when a peer is in reach, the download only when the game needs one. The
//! launch then moves through it; a step that turns out to be unneeded is
//! passed at once, and a step never comes back.

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
    steps: Vec<LaunchStep>,
    current: usize,
}

impl LaunchPlan {
    pub fn new(game_id: &str, peer_in_reach: bool, needs_download: bool) -> Self {
        let steps = [
            peer_in_reach.then_some(LaunchStep::CheckSaves),
            needs_download.then_some(LaunchStep::Download),
            Some(LaunchStep::Prepare),
            Some(LaunchStep::Start),
        ]
        .into_iter()
        .flatten()
        .collect();
        Self {
            game_id: game_id.to_owned(),
            steps,
            current: 0,
        }
    }

    /// korrid's id of the launched game.
    pub fn game_id(&self) -> &str {
        &self.game_id
    }

    /// The launch reached `step`. A step the plan does not list moves to the
    /// next listed one. The current step never moves back.
    pub fn reach(&mut self, step: LaunchStep) {
        if let Some(index) = self.steps.iter().position(|listed| *listed >= step) {
            self.current = self.current.max(index);
        }
    }

    pub fn current(&self) -> LaunchStep {
        self.steps[self.current]
    }

    pub fn surface_steps(&self) -> SurfaceSteps {
        SurfaceSteps {
            labels: self
                .steps
                .iter()
                .map(|step| step.label().to_owned())
                .collect(),
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
}
