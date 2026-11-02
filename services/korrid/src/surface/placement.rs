//! When the surface window is placed. A placement counts as done only once
//! its effect succeeded: a missing or ambiguous window must not block the
//! next try, and a placement asked for during a package switch must not
//! count for the window that replaces it. A failed placement is tried
//! again on the next presentation change or window event, no sooner than
//! its backoff allows. A fresh declaration means the shell's new window has
//! mapped, so it starts over at once.

use std::time::{Duration, Instant};

use korri_surface_treaty::SurfacePackageDisplays;

/// Scope, overlay and launch of one placement.
pub type PlacementKey = (SurfacePackageDisplays, bool, Option<String>);

const FIRST_BACKOFF: Duration = Duration::from_millis(250);
const MAX_BACKOFF: Duration = Duration::from_secs(8);

/// The wait after the `failures`th failure in a row: doubling, bounded.
pub fn backoff(failures: u32) -> Duration {
    FIRST_BACKOFF
        .saturating_mul(1u32 << failures.saturating_sub(1).min(16))
        .min(MAX_BACKOFF)
}

/// What the caller does now.
#[derive(Debug, PartialEq, Eq)]
pub enum Next {
    /// Placed already, or the same placement is in flight or waits.
    Nothing,
    /// Run the effect; report its outcome with this attempt number.
    Attempt(u64),
    /// Within backoff: ask again at this instant.
    RetryAt(Instant),
}

#[derive(Debug, Default)]
pub struct Placement {
    placed: Option<PlacementKey>,
    attempt: Option<(u64, PlacementKey)>,
    /// The key that failed, failures in a row, and the earliest retry.
    failed: Option<(PlacementKey, u32, Instant)>,
    retry_scheduled: bool,
    attempts: u64,
}

impl Placement {
    /// The shell declared a package: its window has mapped. Whatever was
    /// placed, in flight or waiting belongs to the window before it.
    pub fn redeclare(&mut self) {
        self.placed = None;
        self.attempt = None;
        self.failed = None;
        self.retry_scheduled = false;
    }

    /// The placement wanted now.
    pub fn want(&mut self, key: &PlacementKey, now: Instant) -> Next {
        if self.placed.as_ref() == Some(key) {
            return Next::Nothing;
        }
        if self
            .attempt
            .as_ref()
            .is_some_and(|(_, current)| current == key)
        {
            return Next::Nothing;
        }
        if let Some((failed, _, at)) = &self.failed {
            if failed == key && now < *at {
                if self.retry_scheduled {
                    return Next::Nothing;
                }
                self.retry_scheduled = true;
                return Next::RetryAt(*at);
            }
        }
        self.attempts += 1;
        self.placed = None;
        self.retry_scheduled = false;
        self.attempt = Some((self.attempts, key.clone()));
        Next::Attempt(self.attempts)
    }

    /// The effect of `attempt` succeeded. False: a later want or a new
    /// declaration superseded it, and its outcome changes nothing.
    pub fn succeeded(&mut self, attempt: u64) -> bool {
        match self.attempt.take() {
            Some((current, key)) if current == attempt => {
                self.placed = Some(key);
                self.failed = None;
                true
            }
            other => {
                self.attempt = other;
                false
            }
        }
    }

    /// The effect of `attempt` failed. False: superseded.
    pub fn failed(&mut self, attempt: u64, now: Instant) -> bool {
        match self.attempt.take() {
            Some((current, key)) if current == attempt => {
                let failures = match &self.failed {
                    Some((previous, failures, _)) if *previous == key => failures + 1,
                    _ => 1,
                };
                self.failed = Some((key, failures, now + backoff(failures)));
                self.retry_scheduled = false;
                true
            }
            other => {
                self.attempt = other;
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn main() -> PlacementKey {
        (SurfacePackageDisplays::Main, false, None)
    }
    fn all() -> PlacementKey {
        (SurfacePackageDisplays::All, false, None)
    }

    #[test]
    fn backoff_doubles_and_stays_bounded() {
        assert_eq!(backoff(1), Duration::from_millis(250));
        assert_eq!(backoff(2), Duration::from_millis(500));
        assert_eq!(backoff(3), Duration::from_secs(1));
        assert_eq!(backoff(6), Duration::from_secs(8));
        assert_eq!(backoff(40), Duration::from_secs(8));
    }

    #[test]
    fn a_placement_counts_only_after_its_effect_succeeded() {
        let mut placement = Placement::default();
        let now = Instant::now();
        let Next::Attempt(first) = placement.want(&main(), now) else {
            panic!()
        };
        // In flight: asked again, nothing new starts.
        assert_eq!(placement.want(&main(), now), Next::Nothing);
        assert!(placement.succeeded(first));
        assert_eq!(placement.want(&main(), now), Next::Nothing);
    }

    #[test]
    fn a_failed_placement_is_retried_after_its_backoff_not_blocked_forever() {
        let mut placement = Placement::default();
        let start = Instant::now();
        let Next::Attempt(first) = placement.want(&main(), start) else {
            panic!()
        };
        assert!(placement.failed(first, start));
        // An event inside the backoff schedules one retry, not a storm.
        assert_eq!(
            placement.want(&main(), start),
            Next::RetryAt(start + backoff(1))
        );
        assert_eq!(placement.want(&main(), start), Next::Nothing);
        // At the retry time the placement runs again.
        let Next::Attempt(second) = placement.want(&main(), start + backoff(1)) else {
            panic!()
        };
        assert!(placement.failed(second, start + backoff(1)));
        // A second failure in a row waits longer.
        assert_eq!(
            placement.want(&main(), start + backoff(1)),
            Next::RetryAt(start + backoff(1) + backoff(2))
        );
        let later = start + backoff(1) + backoff(2);
        let Next::Attempt(third) = placement.want(&main(), later) else {
            panic!()
        };
        assert!(placement.succeeded(third));
        assert_eq!(placement.want(&main(), later), Next::Nothing);
    }

    #[test]
    fn a_presentation_change_places_at_once_even_while_another_key_waits() {
        let mut placement = Placement::default();
        let now = Instant::now();
        let Next::Attempt(first) = placement.want(&main(), now) else {
            panic!()
        };
        assert!(placement.failed(first, now));
        let overlay = (SurfacePackageDisplays::Main, true, Some("launch-1".into()));
        assert!(matches!(placement.want(&overlay, now), Next::Attempt(_)));
    }

    #[test]
    fn a_redeclaration_after_a_failed_switch_places_at_once_and_ignores_the_old_attempt() {
        let mut placement = Placement::default();
        let now = Instant::now();
        // The switch's placement is asked for while the old window still shows.
        let Next::Attempt(switching) = placement.want(&all(), now) else {
            panic!()
        };
        // The shell maps its new window and declares again before the old
        // attempt answers: the declaration starts over.
        placement.redeclare();
        let Next::Attempt(fresh) = placement.want(&all(), now) else {
            panic!()
        };
        // The old attempt's outcome counts for nothing, success or failure.
        assert!(!placement.succeeded(switching));
        assert!(!placement.failed(switching, now));
        assert!(placement.succeeded(fresh));
        assert_eq!(placement.want(&all(), now), Next::Nothing);

        // A failed placement, then a declaration inside its backoff: no wait.
        let mut placement = Placement::default();
        let Next::Attempt(failed) = placement.want(&all(), now) else {
            panic!()
        };
        assert!(placement.failed(failed, now));
        placement.redeclare();
        assert!(matches!(placement.want(&all(), now), Next::Attempt(_)));
    }
}
