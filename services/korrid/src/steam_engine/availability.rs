//! Why the engine runner cannot run here (contract "Fallback states").
//! Games keep working on the client runner in every case; only a refusal by
//! Steam offers to install and select the client runner for now.
use crate::acquisition::{ArtifactState, ArtifactStatus};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Unavailable {
    /// The host was built with the public, empty engine map.
    MapMissing,
    /// The engine file is not the build the map was made for.
    EngineMismatch,
    /// The pinned engine package is not downloaded on this device.
    PackageNotDownloaded,
    /// Steam refused the pinned engine at sign-in.
    EngineRefused { eresult: i64 },
}

impl Unavailable {
    /// Finished, user-facing copy.
    pub fn reason(&self) -> &'static str {
        match self {
            Self::MapMissing => {
                "This Korri build has no map of Steam's engine. Steam games use the Steam client."
            }
            Self::EngineMismatch => {
                "The Steam engine on this device does not match the one Korri knows. Steam games use the Steam client."
            }
            Self::PackageNotDownloaded => {
                "Korri has not downloaded Steam's engine yet. Steam games use the Steam client until it has."
            }
            Self::EngineRefused { .. } => {
                "Steam no longer accepts the engine version Korri uses. You can use the Steam client until Korri updates."
            }
        }
    }

    pub fn offers_client_runner(&self) -> bool {
        matches!(self, Self::EngineRefused { .. })
    }

    /// The launch refusal code the surface sees.
    pub const CODE: &'static str = "SteamEngineUnavailable";
}

/// The engine package's state, from korrid's artifact status.
pub fn package(status: Option<&ArtifactStatus>) -> Option<Unavailable> {
    match status.map(|status| &status.state) {
        Some(ArtifactState::Ready { .. }) => None,
        _ => Some(Unavailable::PackageNotDownloaded),
    }
}

/// The engine state a host reported, held with the files it was found on.
///
/// A host reports `engine_map_missing` or `engine_mismatch` before it loads
/// the engine, and `engine_refused` at sign-in. korrid keeps the state so it
/// need not start a host to know the engine runner cannot run, but only
/// while it can still be true: a changed engine file or host program clears
/// it (the next host checks again), a host that passed the check
/// (`phase dlopen`) clears a map or file state, a sign-in clears a refusal,
/// and a refusal is asked again after `REFUSED_RECHECK`.
#[derive(Default)]
pub struct EngineHealth {
    state: std::sync::Mutex<Option<Recorded>>,
    /// Moves on every change, so a reader can tell the state changed.
    generation: std::sync::atomic::AtomicU64,
}

struct Recorded {
    reason: Unavailable,
    files: Vec<std::path::PathBuf>,
    stamp: Vec<Option<FileStamp>>,
    at: std::time::Instant,
}

/// What tells one file from its replacement: device, inode, size, mtime.
#[derive(Clone, Debug, Eq, PartialEq)]
struct FileStamp(u64, u64, u64, i64, i64);

fn stamp(files: &[std::path::PathBuf]) -> Vec<Option<FileStamp>> {
    use std::os::unix::fs::MetadataExt;
    files
        .iter()
        .map(|file| {
            std::fs::metadata(file)
                .ok()
                .map(|m| FileStamp(m.dev(), m.ino(), m.size(), m.mtime(), m.mtime_nsec()))
        })
        .collect()
}

/// How long korrid trusts Steam's refusal of the pinned engine.
pub const REFUSED_RECHECK: std::time::Duration = std::time::Duration::from_secs(10 * 60);

impl EngineHealth {
    fn files(plan: &super::host::HostPlan) -> Vec<std::path::PathBuf> {
        vec![plan.host_program.clone(), plan.engine.clone()]
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<Recorded>> {
        self.state.lock().expect("engine health poisoned")
    }

    fn put(&self, state: &mut Option<Recorded>, next: Option<Recorded>) {
        *state = next;
        self.generation
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }

    /// Take one event of a host that ran this plan.
    pub fn observe(&self, plan: &super::host::HostPlan, event: &super::protocol::Event) {
        use super::protocol::Event;
        let reason = match event {
            Event::EngineMapMissing {} => Unavailable::MapMissing,
            Event::EngineMismatch {} => Unavailable::EngineMismatch,
            Event::EngineRefused { eresult } => Unavailable::EngineRefused { eresult: *eresult },
            Event::Phase { phase } if phase == "dlopen" => {
                return self.clear_if(|reason| {
                    matches!(
                        reason,
                        Unavailable::MapMissing | Unavailable::EngineMismatch
                    )
                })
            }
            Event::LogonState {
                logged_on: true, ..
            } => {
                return self.clear_if(|reason| matches!(reason, Unavailable::EngineRefused { .. }))
            }
            _ => return,
        };
        eprintln!("korrid steam engine: unavailable: {reason:?}");
        let files = Self::files(plan);
        let recorded = Recorded {
            reason,
            stamp: stamp(&files),
            files,
            at: std::time::Instant::now(),
        };
        let mut state = self.lock();
        self.put(&mut state, Some(recorded));
    }

    fn clear_if(&self, matches: impl Fn(&Unavailable) -> bool) {
        let mut state = self.lock();
        if state
            .as_ref()
            .is_some_and(|recorded| matches(&recorded.reason))
        {
            eprintln!("korrid steam engine: available again: a host passed its check");
            self.put(&mut state, None);
        }
    }

    /// Clear a state that may no longer hold: its files changed, or a
    /// refusal is old. Cheap (two stats); called on every read.
    pub fn recheck(&self) {
        let mut state = self.lock();
        let stale = state.as_ref().is_some_and(|recorded| {
            stamp(&recorded.files) != recorded.stamp
                || (matches!(recorded.reason, Unavailable::EngineRefused { .. })
                    && recorded.at.elapsed() >= REFUSED_RECHECK)
        });
        if stale {
            eprintln!("korrid steam engine: unavailable state re-checked: the engine or its host changed, or the refusal is old");
            self.put(&mut state, None);
        }
    }

    /// The state for this plan, if it still holds.
    pub fn current(&self, plan: &super::host::HostPlan) -> Option<Unavailable> {
        self.recheck();
        let state = self.lock();
        state
            .as_ref()
            .filter(|recorded| recorded.files == Self::files(plan))
            .map(|recorded| recorded.reason.clone())
    }

    /// The state whatever the plan: for a route, which holds no plan.
    pub fn any(&self) -> Option<Unavailable> {
        self.recheck();
        self.lock().as_ref().map(|recorded| recorded.reason.clone())
    }

    /// Forget the state: the person asked to sign in, so a host checks again.
    pub fn forget(&self) {
        let mut state = self.lock();
        if state.is_some() {
            self.put(&mut state, None);
        }
    }

    pub fn generation(&self) -> u64 {
        self.recheck();
        self.generation.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_state_has_a_reason_and_only_a_refusal_offers_the_client() {
        for state in [
            Unavailable::MapMissing,
            Unavailable::EngineMismatch,
            Unavailable::PackageNotDownloaded,
            Unavailable::EngineRefused { eresult: 5 },
        ] {
            assert!(state.reason().ends_with('.'));
            assert_eq!(
                state.offers_client_runner(),
                matches!(state, Unavailable::EngineRefused { .. })
            );
        }
        assert_eq!(package(None), Some(Unavailable::PackageNotDownloaded));
    }

    fn plan(dir: &std::path::Path) -> super::super::host::HostPlan {
        std::fs::write(dir.join("korri-steam-host"), "host").unwrap();
        std::fs::write(dir.join("steamclient.so"), "engine").unwrap();
        super::super::host::HostPlan {
            host_program: dir.join("korri-steam-host"),
            engine: dir.join("steamclient.so"),
            proton: None,
            account_root: dir.into(),
        }
    }

    /// Device proof 6: the state stayed until korrid restarted.
    #[test]
    fn a_mismatch_holds_until_the_engine_file_changes() {
        use super::super::protocol::Event;
        let temp = tempfile::tempdir().unwrap();
        let plan = plan(temp.path());
        let health = EngineHealth::default();
        health.observe(&plan, &Event::EngineMismatch {});
        assert_eq!(health.current(&plan), Some(Unavailable::EngineMismatch));
        let before = health.generation();
        assert_eq!(health.current(&plan), Some(Unavailable::EngineMismatch));
        // The real engine comes back: another file at the same path.
        std::fs::remove_file(&plan.engine).unwrap();
        std::fs::write(&plan.engine, "the pinned engine").unwrap();
        assert_eq!(health.current(&plan), None);
        assert_ne!(health.generation(), before);
    }

    #[test]
    fn a_host_past_its_check_or_a_sign_in_clears_the_state() {
        use super::super::protocol::Event;
        let temp = tempfile::tempdir().unwrap();
        let plan = plan(temp.path());
        let health = EngineHealth::default();
        health.observe(&plan, &Event::EngineMapMissing {});
        health.observe(
            &plan,
            &Event::Phase {
                phase: "dlopen".into(),
            },
        );
        assert_eq!(health.current(&plan), None);
        health.observe(&plan, &Event::EngineRefused { eresult: 5 });
        // Passing the file check says nothing about Steam's refusal.
        health.observe(
            &plan,
            &Event::Phase {
                phase: "dlopen".into(),
            },
        );
        assert_eq!(
            health.current(&plan),
            Some(Unavailable::EngineRefused { eresult: 5 })
        );
        health.observe(
            &plan,
            &Event::LogonState {
                logged_on: true,
                offline: false,
            },
        );
        assert_eq!(health.any(), None);
        health.observe(&plan, &Event::EngineMismatch {});
        health.forget();
        assert_eq!(health.any(), None);
    }
}
