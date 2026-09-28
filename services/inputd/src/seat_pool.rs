//! Persistent virtual gamepads owned by the production seat receiver.
//!
//! The caller supplies opaque source keys and an explicit count. Keys carry no
//! physical identity or remote authority here; authentication, mapping, input
//! rearming and source-loss detection belong to the receiver coordinator.
//! `NonZeroU8` reflects the existing slot representation, not a persisted setting
//! range or a decision about zero.
//!
//! One pool owns its backend and devices until consuming `stop` or drop. There is
//! idle-only resizing. A coordinator may resize a pool only outside a session (process
//! shutdown can of course drop it at any time). Backends must own their resources
//! and release them on drop, including resources from failed create/destroy calls.
//! A successful backend write must establish the requested full state, even after
//! a prior partial failure. This engine cannot repair a backend's stale diff cache.

use crate::input_seat::{validate_launch_id, GamepadState, SeatBackend, SeatSpec, MAX_SEATS};
use std::num::NonZeroU8;

/// Grounded in the current boot-time seat count, not a configuration schema.
pub const DEFAULT_SEAT_COUNT: NonZeroU8 = NonZeroU8::new(MAX_SEATS).unwrap();

#[derive(Debug, Eq, PartialEq)]
pub enum PoolError {
    NoSeat,
    UnknownSource,
    DisconnectedSource,
    InvalidSession(String),
    SessionAlreadyActive,
    StaleSession,
    Backend(String),
}

struct Binding<K> {
    source: K,
    connected: bool,
}

struct Seat<K> {
    slot: u8,
    binding: Option<Binding<K>>,
    // A failed write may have partially applied. Never deduplicate against an
    // assumed old state, or free such a seat without a successful neutral write.
    known_state: Option<GamepadState>,
}

pub struct SeatPool<K: Eq, B: SeatBackend> {
    backend: Option<B>,
    seats: Vec<Seat<K>>,
    session: Option<String>,
}

impl<K: Eq, B: SeatBackend> SeatPool<K, B> {
    /// Creates all devices once and establishes neutral state before allocation.
    /// On failure, attempts all rollback operations, then drops the owned backend.
    pub fn new(count: NonZeroU8, backend: B) -> Result<Self, PoolError> {
        let mut pool = Self {
            backend: Some(backend),
            seats: Vec::with_capacity(usize::from(count.get())),
            session: None,
        };
        let result = (|| {
            for slot in 1..=count.get() {
                pool.backend
                    .as_mut()
                    .expect("live pool owns backend")
                    .create(&SeatSpec::for_slot(slot))?;
                pool.seats.push(Seat {
                    slot,
                    binding: None,
                    known_state: None,
                });
                pool.write(pool.seats.len() - 1, GamepadState::neutral())?;
            }
            Ok::<(), String>(())
        })();
        if let Err(mut error) = result {
            if let Err(cleanup_error) = pool.cleanup() {
                error.push_str(&format!("; rollback: {cleanup_error}"));
            }
            return Err(PoolError::Backend(error));
        }
        Ok(pool)
    }

    /// Starts reservation scope. Pause/resume must NOT end this scope. Starting
    /// another session cannot replace the current one; end its exact ID first.
    pub fn begin_session(&mut self, session: &str) -> Result<(), PoolError> {
        validate_launch_id(session).map_err(PoolError::InvalidSession)?;
        if self.session.is_some() {
            return Err(PoolError::SessionAlreadyActive);
        }
        self.session = Some(session.to_owned());
        Ok(())
    }

    /// Releases disconnected reservations, retaining connected assignments and
    /// their state. On backend failure, failed reservations remain unavailable
    /// and the session remains bound: retry with the same ID before replacement.
    pub fn end_session(&mut self, session: &str) -> Result<(), PoolError> {
        if self.session.as_deref() != Some(session) {
            return Err(PoolError::StaleSession);
        }
        let mut errors = Vec::new();
        for index in 0..self.seats.len() {
            if self.seats[index]
                .binding
                .as_ref()
                .is_some_and(|binding| !binding.connected)
            {
                match self.write(index, GamepadState::neutral()) {
                    Ok(()) => self.seats[index].binding = None,
                    Err(error) => errors.push(error),
                }
            }
        }
        if !errors.is_empty() {
            return Err(PoolError::Backend(errors.join("; ")));
        }
        self.session = None;
        Ok(())
    }

    /// Idempotent for a connected key. A reserved key reclaims its own slot;
    /// otherwise allocation uses the first free slot and never steals a seat.
    pub fn connect(&mut self, source: K) -> Result<u8, PoolError> {
        let index = if let Some(index) = self.index(&source) {
            if self.seats[index].binding.as_ref().unwrap().connected {
                return Ok(self.seats[index].slot);
            }
            index
        } else {
            self.seats
                .iter()
                .position(|seat| seat.binding.is_none())
                .ok_or(PoolError::NoSeat)?
        };
        self.write(index, GamepadState::neutral())
            .map_err(PoolError::Backend)?;
        self.seats[index].binding = Some(Binding {
            source,
            connected: true,
        });
        Ok(self.seats[index].slot)
    }

    /// Records source loss even if neutralization fails, so subsequent writes
    /// cannot revive it. Retry disconnect (or reconnect) to neutralize that seat.
    /// Outside a session the binding is freed only after successful neutralization.
    pub fn disconnect(&mut self, source: &K) -> Result<u8, PoolError> {
        let index = self.index(source).ok_or(PoolError::UnknownSource)?;
        self.seats[index].binding.as_mut().unwrap().connected = false;
        self.write(index, GamepadState::neutral())
            .map_err(PoolError::Backend)?;
        if self.session.is_none() {
            self.seats[index].binding = None;
        }
        Ok(self.seats[index].slot)
    }

    /// Accepts only explicitly connected sources. Identical successful writes
    /// are deduplicated. An error leaves the slot occupied and its state unknown.
    pub fn write_state(&mut self, source: &K, state: GamepadState) -> Result<u8, PoolError> {
        let index = self.index(source).ok_or(PoolError::UnknownSource)?;
        if !self.seats[index].binding.as_ref().unwrap().connected {
            return Err(PoolError::DisconnectedSource);
        }
        self.write(index, state).map_err(PoolError::Backend)?;
        Ok(self.seats[index].slot)
    }

    pub fn count(&self) -> u8 {
        self.seats.len() as u8
    }

    pub fn session(&self) -> Option<&str> {
        self.session.as_deref()
    }

    pub fn slot(&self, source: &K) -> Option<u8> {
        self.index(source).map(|index| self.seats[index].slot)
    }

    /// Retains every binding. Routing changes must not end reservation scope.
    pub fn neutralize(&mut self) -> Result<(), PoolError> {
        let mut errors = Vec::new();
        for index in 0..self.seats.len() {
            if let Err(error) = self.write(index, GamepadState::neutral()) {
                errors.push(error);
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(PoolError::Backend(errors.join("; ")))
        }
    }

    /// Keep surviving devices and assignments. On failure the caller must retire
    /// this pool: kernel effects may be partial and no count ack is safe.
    pub fn resize(&mut self, count: NonZeroU8) -> Result<(), PoolError> {
        if self.session.is_some() {
            return Err(PoolError::SessionAlreadyActive);
        }
        if count.get() == self.count() {
            return Ok(());
        }
        self.neutralize()?;
        while self.seats.len() > usize::from(count.get()) {
            let slot = self.seats.last().unwrap().slot;
            self.backend
                .as_mut()
                .expect("live pool owns backend")
                .destroy(slot)
                .map_err(PoolError::Backend)?;
            self.seats.pop();
        }
        while self.seats.len() < usize::from(count.get()) {
            let slot = self.count() + 1;
            self.backend
                .as_mut()
                .expect("live pool owns backend")
                .create(&SeatSpec::for_slot(slot))
                .map_err(PoolError::Backend)?;
            self.seats.push(Seat {
                slot,
                binding: None,
                known_state: None,
            });
            self.write(self.seats.len() - 1, GamepadState::neutral())
                .map_err(PoolError::Backend)?;
        }
        Ok(())
    }

    /// Finite ownership: neutralize all seats, attempt every destroy in reverse
    /// order, and drop the backend even on error. No device survives intentionally.
    pub fn stop(mut self) -> Result<(), PoolError> {
        self.cleanup().map_err(PoolError::Backend)
    }

    fn index(&self, source: &K) -> Option<usize> {
        self.seats.iter().position(|seat| {
            seat.binding
                .as_ref()
                .is_some_and(|binding| &binding.source == source)
        })
    }

    fn write(&mut self, index: usize, state: GamepadState) -> Result<(), String> {
        let seat = &mut self.seats[index];
        if seat.known_state == Some(state) {
            return Ok(());
        }
        seat.known_state = None;
        self.backend
            .as_mut()
            .expect("live pool owns backend")
            .write_state(seat.slot, state)?;
        seat.known_state = Some(state);
        Ok(())
    }

    fn cleanup(&mut self) -> Result<(), String> {
        if self.backend.is_none() {
            return Ok(());
        }
        let mut errors = Vec::new();
        for index in 0..self.seats.len() {
            if let Err(error) = self.write(index, GamepadState::neutral()) {
                errors.push(error);
            }
        }
        let mut backend = self.backend.take().expect("live pool owns backend");
        for seat in self.seats.drain(..).rev() {
            if let Err(error) = backend.destroy(seat.slot) {
                errors.push(error);
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }
}

impl<K: Eq, B: SeatBackend> Drop for SeatPool<K, B> {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}
