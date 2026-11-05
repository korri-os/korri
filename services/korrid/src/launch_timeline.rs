//! One journal line per launch: how long each stage took, from the launch
//! request korrid received to the game unit started.
//!
//! A launch crosses several requests (the surface's press, the save check,
//! the download gate, the start), so its timeline is kept here by game id
//! until the start finishes and writes the line. A stage that runs without a
//! begun timeline (the CLI has no press) begins one, so its line still shows
//! every stage korrid saw.

use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

/// A timeline that no start finished in this time was abandoned (a check
/// without a launch, a refused download). The next stage begins a new one.
const ABANDONED_AFTER: Duration = Duration::from_secs(120);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LaunchStage {
    /// The save check, including the peers asked before the launch.
    SavesCheck,
    /// The download gate: missing content checked, and downloaded when asked.
    Downloads,
    /// The launched game's route resolved: the library entry, its runner and
    /// its content.
    Catalog,
    /// Launch settings and device limits resolved.
    Settings,
    /// The runner plugin's launch callback, which declares the command.
    PluginLaunch,
    /// The game unit started (`systemd-run`).
    UnitStart,
}

impl LaunchStage {
    fn key(self) -> &'static str {
        match self {
            Self::SavesCheck => "saves_check",
            Self::Downloads => "downloads",
            Self::Catalog => "catalog",
            Self::Settings => "settings",
            Self::PluginLaunch => "plugin_launch",
            Self::UnitStart => "unit_start",
        }
    }
}

struct Timeline {
    started: Instant,
    stages: Vec<(LaunchStage, Duration)>,
}

impl Timeline {
    fn new(started: Instant) -> Self {
        Self {
            started,
            stages: Vec::new(),
        }
    }

    fn line(&self, game_id: &str, launch_id: Option<&str>, outcome: &str, now: Instant) -> String {
        let total = now.saturating_duration_since(self.started);
        let staged: Duration = self.stages.iter().map(|(_, elapsed)| *elapsed).sum();
        let stages = self
            .stages
            .iter()
            .map(|(stage, elapsed)| format!("\"{}\": {}", stage.key(), elapsed.as_millis()))
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "<6>korrid: LaunchTimeline {{ game_id: {game_id}, launch_id: {}, outcome: {outcome}, total_ms: {}, stages_ms: {{{stages}}}, between_stages_ms: {} }}",
            launch_id.unwrap_or("none"),
            total.as_millis(),
            total.saturating_sub(staged).as_millis(),
        )
    }
}

fn timelines() -> &'static Mutex<HashMap<String, Timeline>> {
    static TIMELINES: OnceLock<Mutex<HashMap<String, Timeline>>> = OnceLock::new();
    TIMELINES.get_or_init(Mutex::default)
}

/// korrid received the request to launch this game.
pub(crate) fn begin(game_id: &str) {
    if let Ok(mut timelines) = timelines().lock() {
        timelines.insert(game_id.to_owned(), Timeline::new(Instant::now()));
    }
}

/// One stage of this game's launch ended after `elapsed`.
pub(crate) fn record(game_id: &str, stage: LaunchStage, elapsed: Duration) {
    let now = Instant::now();
    let Ok(mut timelines) = timelines().lock() else {
        return;
    };
    let stage_started = now.checked_sub(elapsed).unwrap_or(now);
    let timeline = timelines
        .entry(game_id.to_owned())
        .or_insert_with(|| Timeline::new(stage_started));
    if now.saturating_duration_since(timeline.started) > ABANDONED_AFTER {
        *timeline = Timeline::new(stage_started);
    }
    // A stage measured in two places (the catalog checks before and inside
    // the start) is one stage of the launch.
    match timeline
        .stages
        .iter_mut()
        .find(|(known, _)| *known == stage)
    {
        Some((_, total)) => *total += elapsed,
        None => timeline.stages.push((stage, elapsed)),
    }
}

/// Time `run` as one stage of this game's launch.
pub(crate) fn measure<T>(game_id: &str, stage: LaunchStage, run: impl FnOnce() -> T) -> T {
    let started = Instant::now();
    let value = run();
    record(game_id, stage, started.elapsed());
    value
}

/// The start ended: write the journal line and forget the timeline.
/// `outcome` is `started` or the refusal's failure code.
pub(crate) fn finish(game_id: &str, launch_id: Option<&str>, outcome: &str) {
    if let Some(line) = take_line(game_id, launch_id, outcome) {
        eprintln!("{line}");
    }
}

fn take_line(game_id: &str, launch_id: Option<&str>, outcome: &str) -> Option<String> {
    let now = Instant::now();
    let timeline = timelines().lock().ok()?.remove(game_id)?;
    Some(timeline.line(game_id, launch_id, outcome, now))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_lists_each_stage_in_order_and_the_time_between_stages() {
        let started = Instant::now();
        let timeline = Timeline {
            started,
            stages: vec![
                (LaunchStage::SavesCheck, Duration::from_millis(40)),
                (LaunchStage::Catalog, Duration::from_millis(15)),
                (LaunchStage::UnitStart, Duration::from_millis(5)),
            ],
        };
        assert_eq!(
            timeline.line(
                "anguna",
                Some("launch-1"),
                "started",
                started + Duration::from_millis(100)
            ),
            "<6>korrid: LaunchTimeline { game_id: anguna, launch_id: launch-1, outcome: started, total_ms: 100, stages_ms: {\"saves_check\": 40, \"catalog\": 15, \"unit_start\": 5}, between_stages_ms: 40 }"
        );
    }

    #[test]
    fn a_stage_without_a_begun_launch_begins_one() {
        record(
            "launch-timeline-cli",
            LaunchStage::SavesCheck,
            Duration::from_millis(7),
        );
        measure("launch-timeline-cli", LaunchStage::Catalog, || ());
        record(
            "launch-timeline-cli",
            LaunchStage::SavesCheck,
            Duration::from_millis(2),
        );
        let line = take_line("launch-timeline-cli", None, "HostGameNotFound").unwrap();
        assert!(line.contains("outcome: HostGameNotFound"), "{line}");
        assert!(line.contains("\"saves_check\": 9, \"catalog\": "), "{line}");
        assert!(take_line("launch-timeline-cli", None, "started").is_none());
    }

    #[test]
    fn begin_drops_an_earlier_unfinished_launch_of_the_same_game() {
        record(
            "launch-timeline-again",
            LaunchStage::SavesCheck,
            Duration::from_millis(9),
        );
        begin("launch-timeline-again");
        record(
            "launch-timeline-again",
            LaunchStage::UnitStart,
            Duration::from_millis(3),
        );
        let line = take_line("launch-timeline-again", Some("l"), "started").unwrap();
        assert!(line.contains("stages_ms: {\"unit_start\": 3}"), "{line}");
    }
}
