//! Process-local, caller-bound launch reservations. No durable schema.
use crate::{PendingLaunch, PendingLaunchPhase, RpcFailure, SessionPrepared};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc, Mutex,
    },
};

const RESERVED: u8 = 0;
const PREPARING: u8 = 1;
const COMMITTING: u8 = 2;
const CANCELLED: u8 = 3;
const CANCELLED_COMMITTING: u8 = 4;

pub(super) struct LaunchReservation {
    pub identity: SessionPrepared,
    caller: String,
    pub person: Option<String>,
    phase: AtomicU8,
}

impl LaunchReservation {
    pub fn is_cancelled(&self) -> bool {
        matches!(
            self.phase.load(Ordering::SeqCst),
            CANCELLED | CANCELLED_COMMITTING
        )
    }

    /// Called under the session transition lock immediately before effects.
    pub fn commit(&self) -> Result<(), RpcFailure> {
        self.phase
            .compare_exchange(PREPARING, COMMITTING, Ordering::SeqCst, Ordering::SeqCst)
            .map(|_| ())
            .map_err(|_| cancelled())
    }
}

#[derive(Clone, Default)]
pub(super) struct LaunchReservations(Arc<Mutex<HashMap<String, Arc<LaunchReservation>>>>);

impl LaunchReservations {
    pub fn reserve(&self, game_id: &str, caller: &str, person: Option<&str>) -> SessionPrepared {
        let identity = SessionPrepared {
            game_id: game_id.into(),
            launch_id: crate::generate_launch_id(),
        };
        let reservation = Arc::new(LaunchReservation {
            identity: identity.clone(),
            caller: caller.into(),
            person: person.map(str::to_owned),
            phase: AtomicU8::new(RESERVED),
        });
        self.0
            .lock()
            .expect("reservation mutex poisoned")
            .insert(identity.launch_id.clone(), reservation);
        identity
    }

    /// Copy only this caller's facts, preserving multiplicity. When both locks
    /// are needed, native transition authority precedes the reservation mutex;
    /// never retain this mutex while entering native authority.
    pub fn snapshot(&self, caller: &str, person: Option<&str>) -> Vec<PendingLaunch> {
        let pending = self.0.lock().expect("reservation mutex poisoned");
        pending
            .values()
            .filter(|reservation| {
                reservation.caller == caller && reservation.person.as_deref() == person
            })
            .filter_map(|reservation| {
                let phase = match reservation.phase.load(Ordering::SeqCst) {
                    RESERVED => PendingLaunchPhase::Reserved,
                    PREPARING => PendingLaunchPhase::Preparing,
                    COMMITTING => PendingLaunchPhase::Committing,
                    CANCELLED_COMMITTING => PendingLaunchPhase::Cancelling,
                    CANCELLED => return None,
                    _ => unreachable!("reservation phase is internal"),
                };
                Some(PendingLaunch {
                    session: reservation.identity.clone(),
                    phase,
                })
            })
            .collect()
    }

    pub fn start(
        &self,
        game_id: &str,
        launch_id: &str,
        caller: &str,
        person: Option<&str>,
    ) -> Result<Arc<LaunchReservation>, RpcFailure> {
        let pending = self.0.lock().expect("reservation mutex poisoned");
        let reservation = pending.get(launch_id).ok_or_else(stale)?;
        if reservation.caller != caller || reservation.person.as_deref() != person {
            return Err(denied());
        }
        if reservation.identity.game_id != game_id {
            return Err(stale());
        }
        reservation
            .phase
            .compare_exchange(RESERVED, PREPARING, Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|_| stale())?;
        Ok(reservation.clone())
    }

    /// None means no pending request: the caller must use ordinary exact stop.
    /// true means effects have started and cancellation must report Pending.
    pub fn cancel(
        &self,
        launch_id: &str,
        caller: &str,
        person: Option<&str>,
    ) -> Result<Option<bool>, RpcFailure> {
        let mut pending = self.0.lock().expect("reservation mutex poisoned");
        let Some(reservation) = pending.get(launch_id) else {
            return Ok(None);
        };
        if reservation.caller != caller || reservation.person.as_deref() != person {
            return Err(denied());
        }
        let committing = loop {
            let prior = reservation.phase.load(Ordering::SeqCst);
            let committing = prior == COMMITTING || prior == CANCELLED_COMMITTING;
            let next = if committing {
                CANCELLED_COMMITTING
            } else {
                CANCELLED
            };
            if reservation
                .phase
                .compare_exchange(prior, next, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                break committing;
            }
        };
        if !committing {
            pending.remove(launch_id);
        }
        Ok(Some(committing))
    }

    /// Exact native completion may precede the worker's acknowledgement.
    /// Retire only reservations whose effects really crossed the commit gate.
    pub fn retire_completed(&self, launch_id: &str) {
        let mut pending = self.0.lock().expect("reservation mutex poisoned");
        if pending.get(launch_id).is_some_and(|reservation| {
            matches!(
                reservation.phase.load(Ordering::SeqCst),
                COMMITTING | CANCELLED_COMMITTING
            )
        }) {
            pending.remove(launch_id);
        }
    }

    /// Serialize completion with cancel. A cancelled committing request must
    /// exact-stop after releasing the session transition lock.
    pub fn finish(&self, reservation: &LaunchReservation) -> Option<bool> {
        let mut pending = self.0.lock().expect("reservation mutex poisoned");
        pending.remove(&reservation.identity.launch_id);
        match reservation.phase.load(Ordering::SeqCst) {
            CANCELLED => Some(false),
            CANCELLED_COMMITTING => Some(true),
            _ => None,
        }
    }
}

pub(super) fn cancelled() -> RpcFailure {
    RpcFailure {
        code: "LaunchCancelled".into(),
        message: "the exact launch startup was cancelled".into(),
    }
}
fn stale() -> RpcFailure {
    RpcFailure {
        code: "StaleLaunchIdentity".into(),
        message: "the exact launch reservation is unavailable or already started".into(),
    }
}
fn denied() -> RpcFailure {
    RpcFailure {
        code: "AuthorizationDenied".into(),
        message: "the launch reservation belongs to another caller".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_projection_uses_the_producers_caller_and_person_binding() {
        let reservations = LaunchReservations::default();
        let person = "1".repeat(64);
        let other_person = "2".repeat(64);
        let prepared = reservations.reserve("one", "local-capability", Some(&person));
        assert_eq!(
            reservations.snapshot("local-capability", Some(&person))[0]
                .session
                .launch_id,
            prepared.launch_id
        );
        assert!(reservations
            .snapshot("local-capability", Some(&other_person))
            .is_empty());
        assert!(reservations.snapshot("local-capability", None).is_empty());
        assert!(reservations
            .snapshot("foreign-capability", Some(&person))
            .is_empty());
        assert_eq!(
            reservations
                .cancel(&prepared.launch_id, "local-capability", Some(&other_person))
                .unwrap_err()
                .code,
            "AuthorizationDenied"
        );
        assert_eq!(
            reservations
                .snapshot("local-capability", Some(&person))
                .len(),
            1
        );
    }
}
